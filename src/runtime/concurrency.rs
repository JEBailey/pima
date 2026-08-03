use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex, Weak, mpsc},
};

mod transport;

pub(crate) use transport::{TransportError, TransportValue};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContextTransferMode {
    #[default]
    Copy,
    Move,
    Share,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RemoteNamespaceHandle {
    hub: u64,
    object: u64,
}

impl RemoteNamespaceHandle {
    pub(crate) fn new(hub: u64, object: u64) -> Self {
        Self { hub, object }
    }

    pub(crate) fn hub(self) -> u64 {
        self.hub
    }

    pub(crate) fn object(self) -> u64 {
        self.object
    }
}

#[derive(Clone, Debug)]
pub struct TaskHandle {
    hub: u64,
    task: u64,
    _lease: Arc<TaskLease>,
}

#[derive(Debug)]
struct TaskLease {
    task: u64,
    hub: Weak<HubShared>,
}

impl Drop for TaskLease {
    fn drop(&mut self) {
        let Some(hub) = self.hub.upgrade() else {
            return;
        };
        let mut state = hub.state.lock().unwrap_or_else(|error| error.into_inner());
        state.tasks.remove(&self.task);
    }
}

impl PartialEq for TaskHandle {
    fn eq(&self, other: &Self) -> bool {
        self.hub == other.hub && self.task == other.task
    }
}

impl Eq for TaskHandle {}

impl std::hash::Hash for TaskHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hub.hash(state);
        self.task.hash(state);
    }
}

#[derive(Clone, Debug)]
pub struct RemoteBlueprint {
    pub(crate) preamble: std::sync::Arc<str>,
    pub(crate) source: std::sync::Arc<str>,
    pub(crate) public_functions: Vec<std::sync::Arc<str>>,
}

#[derive(Debug)]
pub(crate) enum RemoteOperation {
    Read {
        member: std::sync::Arc<str>,
    },
    Call {
        member: std::sync::Arc<str>,
        arguments: Vec<TransportValue>,
    },
    Stop,
}

#[derive(Debug)]
pub(crate) struct RemoteRequest {
    pub(crate) operation: RemoteOperation,
    pub(crate) reply: RemoteReply,
}

#[derive(Debug)]
pub(crate) enum RemoteReply {
    Blocking(mpsc::Sender<Result<TransportValue, TransportError>>),
    Task(TaskHandle),
}

impl TaskHandle {
    fn new(hub: u64, task: u64, shared: &Arc<HubShared>) -> Self {
        Self {
            hub,
            task,
            _lease: Arc::new(TaskLease {
                task,
                hub: Arc::downgrade(shared),
            }),
        }
    }

    pub(crate) fn hub(&self) -> u64 {
        self.hub
    }

    pub(crate) fn task(&self) -> u64 {
        self.task
    }
}

#[derive(Debug)]
enum TaskState {
    Pending,
    Complete(Result<TransportValue, TransportError>),
}

#[derive(Debug, Default)]
struct HubState {
    #[allow(dead_code)] // consumed by remote blueprint allocation in the next phase
    next_object: u64,
    next_task: u64,
    remotes: HashMap<u64, RemoteEntry>,
    tasks: HashMap<u64, TaskState>,
}

#[derive(Debug)]
struct RemoteEntry {
    alive: bool,
    public_functions: Vec<std::sync::Arc<str>>,
    sender: Option<mpsc::Sender<RemoteRequest>>,
}

#[derive(Debug)]
pub(crate) struct ConcurrencyHub {
    id: u64,
    shared: Arc<HubShared>,
}

#[derive(Debug)]
struct HubShared {
    state: Mutex<HubState>,
    completed: Condvar,
}

impl ConcurrencyHub {
    pub(crate) fn new() -> Self {
        static NEXT_HUB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT_HUB.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            shared: Arc::new(HubShared {
                state: Mutex::new(HubState::default()),
                completed: Condvar::new(),
            }),
        }
    }

    #[allow(dead_code)] // safe identity reservation precedes worker blueprint support
    pub(crate) fn reserve_remote(&self) -> RemoteNamespaceHandle {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned");
        let object = state.next_object;
        state.next_object += 1;
        state.remotes.insert(
            object,
            RemoteEntry {
                alive: true,
                public_functions: Vec::new(),
                sender: None,
            },
        );
        RemoteNamespaceHandle::new(self.id, object)
    }

    pub(crate) fn register_remote(
        &self,
        sender: mpsc::Sender<RemoteRequest>,
        public_functions: Vec<std::sync::Arc<str>>,
    ) -> RemoteNamespaceHandle {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned");
        let object = state.next_object;
        state.next_object += 1;
        state.remotes.insert(
            object,
            RemoteEntry {
                alive: true,
                public_functions,
                sender: Some(sender),
            },
        );
        RemoteNamespaceHandle::new(self.id, object)
    }

    pub(crate) fn remote_alive(&self, handle: RemoteNamespaceHandle) -> Result<bool, &'static str> {
        self.validate_hub(handle.hub())?;
        Ok(self
            .shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned")
            .remotes
            .get(&handle.object())
            .map(|remote| remote.alive)
            .unwrap_or(false))
    }

    pub(crate) fn stop_remote(&self, handle: RemoteNamespaceHandle) -> Result<(), &'static str> {
        self.validate_hub(handle.hub())?;
        let mut state = self
            .shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned");
        let remote = state
            .remotes
            .remove(&handle.object())
            .ok_or("unknown remote object")?;
        if let Some(sender) = remote.sender {
            let (reply, _) = mpsc::channel();
            let _ = sender.send(RemoteRequest {
                operation: RemoteOperation::Stop,
                reply: RemoteReply::Blocking(reply),
            });
        }
        Ok(())
    }

    pub(crate) fn remote_member_is_function(
        &self,
        handle: RemoteNamespaceHandle,
        member: &str,
    ) -> Result<bool, &'static str> {
        self.validate_hub(handle.hub())?;
        self.shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned")
            .remotes
            .get(&handle.object())
            .map(|remote| {
                remote
                    .public_functions
                    .iter()
                    .any(|candidate| candidate.as_ref() == member)
            })
            .ok_or("unknown remote object")
    }

    pub(crate) fn request_remote_async(
        &self,
        handle: RemoteNamespaceHandle,
        operation: RemoteOperation,
    ) -> Result<TaskHandle, &'static str> {
        self.validate_hub(handle.hub())?;
        let sender = {
            let state = self
                .shared
                .state
                .lock()
                .expect("concurrency hub mutex poisoned");
            let remote = state
                .remotes
                .get(&handle.object())
                .ok_or("unknown remote object")?;
            if !remote.alive {
                return Err("remote object is stopped");
            }
            remote
                .sender
                .clone()
                .ok_or("remote worker is unavailable")?
        };
        let task = self.create_task();
        if sender
            .send(RemoteRequest {
                operation,
                reply: RemoteReply::Task(task.clone()),
            })
            .is_err()
        {
            let _ = self.complete_task(
                &task,
                Err(TransportError {
                    types: vec![
                        std::sync::Arc::from("error"),
                        std::sync::Arc::from("remote_error"),
                        std::sync::Arc::from("stopped"),
                    ],
                    message: std::sync::Arc::from("remote worker is stopped"),
                }),
            );
        }
        Ok(task)
    }

    pub(crate) fn create_task(&self) -> TaskHandle {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned");
        let task = state.next_task;
        state.next_task += 1;
        state.tasks.insert(task, TaskState::Pending);
        TaskHandle::new(self.id, task, &self.shared)
    }

    pub(crate) fn complete_task(
        &self,
        handle: &TaskHandle,
        result: Result<TransportValue, TransportError>,
    ) -> Result<(), &'static str> {
        self.validate_hub(handle.hub())?;
        let mut state = self
            .shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned");
        let task = state.tasks.get_mut(&handle.task()).ok_or("unknown task")?;
        if matches!(task, TaskState::Complete(_)) {
            return Err("task is already complete");
        }
        *task = TaskState::Complete(result);
        self.shared.completed.notify_all();
        Ok(())
    }

    pub(crate) fn task_complete(&self, handle: &TaskHandle) -> Result<bool, &'static str> {
        self.validate_hub(handle.hub())?;
        self.shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned")
            .tasks
            .get(&handle.task())
            .map(|task| matches!(task, TaskState::Complete(_)))
            .ok_or("unknown task")
    }

    pub(crate) fn await_task(
        &self,
        handle: &TaskHandle,
    ) -> Result<Result<TransportValue, TransportError>, &'static str> {
        self.validate_hub(handle.hub())?;
        let mut state = self
            .shared
            .state
            .lock()
            .expect("concurrency hub mutex poisoned");
        loop {
            match state.tasks.get(&handle.task()) {
                Some(TaskState::Pending) => {
                    state = self
                        .shared
                        .completed
                        .wait(state)
                        .expect("concurrency hub mutex poisoned");
                }
                Some(TaskState::Complete(result)) => return Ok(result.clone()),
                None => return Err("unknown future"),
            }
        }
    }

    fn validate_hub(&self, hub: u64) -> Result<(), &'static str> {
        (hub == self.id)
            .then_some(())
            .ok_or("handle belongs to another concurrency hub")
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;

    #[test]
    fn future_results_can_be_awaited_repeatedly() {
        let hub = Arc::new(ConcurrencyHub::new());
        let task = hub.create_task();
        let worker_hub = Arc::clone(&hub);
        let worker_task = task.clone();
        std::thread::spawn(move || {
            worker_hub
                .complete_task(&worker_task, Ok(TransportValue::Integer(42)))
                .expect("worker should complete task");
        })
        .join()
        .expect("worker should not panic");

        assert!(hub.task_complete(&task).expect("task should exist"));
        assert_eq!(
            hub.await_task(&task).expect("future should be awaitable"),
            Ok(TransportValue::Integer(42))
        );
        assert_eq!(
            hub.await_task(&task)
                .expect("future should remain awaitable"),
            Ok(TransportValue::Integer(42))
        );
    }

    #[test]
    fn awaiting_a_pending_future_waits_for_completion() {
        let hub = Arc::new(ConcurrencyHub::new());
        let task = hub.create_task();
        let worker_hub = Arc::clone(&hub);
        let worker_task = task.clone();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            worker_hub
                .complete_task(&worker_task, Ok(TransportValue::Boolean(true)))
                .expect("worker should complete task");
        });

        assert_eq!(
            hub.await_task(&task).expect("future should be awaitable"),
            Ok(TransportValue::Boolean(true))
        );
        worker.join().expect("worker should not panic");
    }

    #[test]
    fn dropping_the_last_future_handle_releases_its_result() {
        let hub = ConcurrencyHub::new();
        let task = hub.create_task();
        hub.complete_task(&task, Ok(TransportValue::Integer(42)))
            .expect("task should complete");
        assert_eq!(hub.shared.state.lock().expect("hub state").tasks.len(), 1);

        drop(task);

        assert!(hub.shared.state.lock().expect("hub state").tasks.is_empty());
    }

    #[test]
    fn remote_handles_are_scoped_to_their_hub() {
        let first = ConcurrencyHub::new();
        let second = ConcurrencyHub::new();
        let remote = first.reserve_remote();

        assert_eq!(first.remote_alive(remote), Ok(true));
        assert!(second.remote_alive(remote).is_err());
        first.stop_remote(remote).expect("remote should stop");
        assert_eq!(first.remote_alive(remote), Ok(false));
        assert!(
            first
                .shared
                .state
                .lock()
                .expect("hub state")
                .remotes
                .is_empty()
        );
    }
}
