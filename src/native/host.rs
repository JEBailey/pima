use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};

use crate::runtime::{TcpConnectionId, TcpListenerId};

#[derive(Debug)]
pub(crate) struct HostResources {
    working_directory: PathBuf,
    network: std::sync::Arc<std::sync::Mutex<NetworkResources>>,
    concurrency: std::sync::Arc<crate::runtime::ConcurrencyHub>,
}

#[derive(Debug, Default)]
pub(crate) struct NetworkResources {
    listeners: Vec<Option<std::sync::Arc<TcpListener>>>,
    connections: Vec<Option<std::sync::Arc<std::sync::Mutex<TcpStream>>>>,
}

impl HostResources {
    pub(crate) fn new(working_directory: PathBuf) -> Self {
        Self::with_concurrency(
            working_directory,
            std::sync::Arc::new(crate::runtime::ConcurrencyHub::new()),
            std::sync::Arc::new(std::sync::Mutex::new(NetworkResources::default())),
        )
    }

    pub(crate) fn with_concurrency(
        working_directory: PathBuf,
        concurrency: std::sync::Arc<crate::runtime::ConcurrencyHub>,
        network: std::sync::Arc<std::sync::Mutex<NetworkResources>>,
    ) -> Self {
        Self {
            working_directory,
            network,
            concurrency,
        }
    }

    pub(crate) fn remote_alive(
        &self,
        handle: crate::runtime::RemoteNamespaceHandle,
    ) -> Result<bool, String> {
        self.concurrency.remote_alive(handle).map_err(str::to_owned)
    }

    pub(crate) fn stop_remote(
        &self,
        handle: crate::runtime::RemoteNamespaceHandle,
    ) -> Result<(), String> {
        self.concurrency.stop_remote(handle).map_err(str::to_owned)
    }

    pub(crate) fn make_remote(
        &self,
        blueprint: crate::runtime::RemoteBlueprint,
        context: Vec<(std::sync::Arc<str>, crate::runtime::TransportValue)>,
    ) -> Result<crate::runtime::RemoteNamespaceHandle, String> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = self
            .concurrency
            .register_remote(sender, blueprint.public_functions.clone());
        let working_directory = self.working_directory.clone();
        let concurrency = self.concurrency.clone();
        let network = self.network.clone();
        let (initialization, initialized) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name(format!("pima-remote-{}", handle.object()))
            .spawn(move || {
                let mut interpreter = crate::engine::Interpreter::new_remote_worker(
                    working_directory,
                    concurrency.clone(),
                    network,
                );
                for (name, value) in context {
                    let value = interpreter.vm.import_transport(value);
                    interpreter.vm_session_globals.insert(name, value);
                }
                let initialization_source = format!(
                    "{}\nval __remote [new {}]\n",
                    blueprint.preamble, blueprint.source
                );
                let outcome = interpreter.run_source("<remote-init>", &initialization_source);
                if !outcome.is_success() {
                    let message = outcome
                        .diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; ");
                    let _ = initialization.send(Err(message));
                    return;
                }
                let _ = initialization.send(Ok(()));

                while let Ok(request) = receiver.recv() {
                    match request.operation {
                        crate::runtime::RemoteOperation::Stop => {
                            deliver_remote_reply(
                                &concurrency,
                                request.reply,
                                Ok(crate::runtime::TransportValue::Unit),
                            );
                            break;
                        }
                        crate::runtime::RemoteOperation::Read { member } => {
                            let source = format!("__remote.{member}\n");
                            let result = run_remote_request(&mut interpreter, &source, Vec::new());
                            deliver_remote_reply(&concurrency, request.reply, result);
                        }
                        crate::runtime::RemoteOperation::Call { member, arguments } => {
                            let names = (0..arguments.len())
                                .map(|index| format!("__remote_arg_{index}"))
                                .collect::<Vec<_>>();
                            let source = if names.is_empty() {
                                format!("[__remote.{member}]\n")
                            } else {
                                format!("[__remote.{member} {}]\n", names.join(" "))
                            };
                            let result = run_remote_request(&mut interpreter, &source, arguments);
                            deliver_remote_reply(&concurrency, request.reply, result);
                        }
                    }
                }
            })
            .map_err(|error| format!("could not start remote worker: {error}"))?;

        match initialized.recv() {
            Ok(Ok(())) => Ok(handle),
            Ok(Err(message)) => {
                let _ = self.concurrency.stop_remote(handle);
                Err(format!("remote object initialization failed: {message}"))
            }
            Err(_) => {
                let _ = self.concurrency.stop_remote(handle);
                Err("remote worker stopped during initialization".to_owned())
            }
        }
    }

    pub(crate) fn remote_member_is_function(
        &self,
        handle: crate::runtime::RemoteNamespaceHandle,
        member: &str,
    ) -> Result<bool, String> {
        self.concurrency
            .remote_member_is_function(handle, member)
            .map_err(str::to_owned)
    }

    pub(crate) fn future_remote(
        &self,
        handle: crate::runtime::RemoteNamespaceHandle,
        member: std::sync::Arc<str>,
        arguments: Option<Vec<crate::runtime::TransportValue>>,
    ) -> Result<crate::runtime::TaskHandle, String> {
        let operation = match arguments {
            Some(arguments) => crate::runtime::RemoteOperation::Call { member, arguments },
            None => crate::runtime::RemoteOperation::Read { member },
        };
        self.concurrency
            .request_remote_async(handle, operation)
            .map_err(str::to_owned)
    }

    pub(crate) fn task_complete(&self, handle: crate::runtime::TaskHandle) -> Result<bool, String> {
        self.concurrency
            .task_complete(&handle)
            .map_err(str::to_owned)
    }

    pub(crate) fn await_task(
        &self,
        handle: crate::runtime::TaskHandle,
    ) -> Result<Result<crate::runtime::TransportValue, crate::runtime::TransportError>, String>
    {
        self.concurrency.await_task(&handle).map_err(str::to_owned)
    }

    pub(crate) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub(crate) fn listen(&mut self, address: &str, port: u16) -> Result<TcpListenerId, String> {
        let listener = TcpListener::bind((address, port))
            .map_err(|error| format!("could not listen on {address}:{port}: {error}"))?;
        let mut network = self
            .network
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let id = TcpListenerId(network.listeners.len() as u32);
        network.listeners.push(Some(std::sync::Arc::new(listener)));
        Ok(id)
    }

    pub(crate) fn accept(&mut self, listener: TcpListenerId) -> Result<TcpConnectionId, String> {
        let listener = self
            .network
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .listeners
            .get(listener.0 as usize)
            .and_then(Clone::clone)
            .ok_or_else(|| "TCP listener is closed".to_owned())?;
        let (connection, _) = listener
            .accept()
            .map_err(|error| format!("could not accept TCP connection: {error}"))?;
        let mut network = self
            .network
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let id = TcpConnectionId(network.connections.len() as u32);
        network
            .connections
            .push(Some(std::sync::Arc::new(std::sync::Mutex::new(connection))));
        Ok(id)
    }

    pub(crate) fn read(
        &mut self,
        connection: TcpConnectionId,
        maximum: usize,
    ) -> Result<String, String> {
        let connection = self.connection(connection)?;
        let mut connection = connection.lock().unwrap_or_else(|error| error.into_inner());
        let mut bytes = vec![0; maximum];
        let count = connection
            .read(&mut bytes)
            .map_err(|error| format!("could not read TCP connection: {error}"))?;
        String::from_utf8(bytes[..count].to_vec())
            .map_err(|_| "TCP read was not valid UTF-8".to_owned())
    }

    pub(crate) fn write(&mut self, connection: TcpConnectionId, text: &str) -> Result<(), String> {
        self.connection(connection)?
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .write_all(text.as_bytes())
            .map_err(|error| format!("could not write TCP connection: {error}"))
    }

    pub(crate) fn set_timeout(
        &mut self,
        connection: TcpConnectionId,
        milliseconds: u64,
    ) -> Result<(), String> {
        let connection = self.connection(connection)?;
        let connection = connection.lock().unwrap_or_else(|error| error.into_inner());
        let timeout = Some(Duration::from_millis(milliseconds));
        connection
            .set_read_timeout(timeout)
            .and_then(|_| connection.set_write_timeout(timeout))
            .map_err(|error| format!("could not set TCP timeout: {error}"))
    }

    pub(crate) fn close_listener(&mut self, listener: TcpListenerId) -> Result<(), String> {
        self.network
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .listeners
            .get_mut(listener.0 as usize)
            .ok_or_else(|| "invalid TCP listener".to_owned())?
            .take()
            .map(|_| ())
            .ok_or_else(|| "TCP listener is already closed".to_owned())
    }

    pub(crate) fn close_connection(&mut self, connection: TcpConnectionId) -> Result<(), String> {
        self.network
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .connections
            .get_mut(connection.0 as usize)
            .ok_or_else(|| "invalid TCP connection".to_owned())?
            .take()
            .map(|_| ())
            .ok_or_else(|| "TCP connection is already closed".to_owned())
    }

    fn connection(
        &self,
        connection: TcpConnectionId,
    ) -> Result<std::sync::Arc<std::sync::Mutex<TcpStream>>, String> {
        self.network
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .connections
            .get(connection.0 as usize)
            .and_then(Clone::clone)
            .ok_or_else(|| "TCP connection is closed".to_owned())
    }
}

fn deliver_remote_reply(
    concurrency: &crate::runtime::ConcurrencyHub,
    reply: crate::runtime::RemoteReply,
    result: Result<crate::runtime::TransportValue, crate::runtime::TransportError>,
) {
    match reply {
        crate::runtime::RemoteReply::Blocking(sender) => {
            let _ = sender.send(result);
        }
        crate::runtime::RemoteReply::Task(task) => {
            let _ = concurrency.complete_task(&task, result);
        }
    }
}

fn run_remote_request(
    interpreter: &mut crate::Interpreter,
    source: &str,
    arguments: Vec<crate::runtime::TransportValue>,
) -> Result<crate::runtime::TransportValue, crate::runtime::TransportError> {
    let names = (0..arguments.len())
        .map(|index| std::sync::Arc::<str>::from(format!("__remote_arg_{index}")))
        .collect::<Vec<_>>();
    for (name, argument) in names.iter().cloned().zip(arguments) {
        let value = interpreter.vm.import_transport(argument);
        interpreter.vm_session_globals.insert(name, value);
    }
    let outcome = interpreter.run_source("<remote-request>", source);
    for name in names {
        interpreter.vm_session_globals.remove(&name);
    }
    if let Some(value) = outcome.value
        && outcome.diagnostics.is_empty()
    {
        return interpreter.vm.export_transport(&value).map_err(|message| {
            crate::runtime::TransportError {
                types: vec![
                    std::sync::Arc::from("error"),
                    std::sync::Arc::from("remote_error"),
                    std::sync::Arc::from("unsendable_value"),
                ],
                message: std::sync::Arc::from(message),
            }
        });
    }
    Err(crate::runtime::TransportError {
        types: vec![
            std::sync::Arc::from("error"),
            std::sync::Arc::from("remote_error"),
            std::sync::Arc::from("worker_failure"),
        ],
        message: std::sync::Arc::from(
            outcome
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ),
    })
}
