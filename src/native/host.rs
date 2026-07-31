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
    listeners: Vec<Option<TcpListener>>,
    connections: Vec<Option<TcpStream>>,
}

impl HostResources {
    pub(crate) fn new(working_directory: PathBuf) -> Self {
        Self {
            working_directory,
            listeners: Vec::new(),
            connections: Vec::new(),
        }
    }

    pub(crate) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub(crate) fn listen(&mut self, address: &str, port: u16) -> Result<TcpListenerId, String> {
        let listener = TcpListener::bind((address, port))
            .map_err(|error| format!("could not listen on {address}:{port}: {error}"))?;
        let id = TcpListenerId(self.listeners.len() as u32);
        self.listeners.push(Some(listener));
        Ok(id)
    }

    pub(crate) fn accept(&mut self, listener: TcpListenerId) -> Result<TcpConnectionId, String> {
        let listener = self
            .listeners
            .get(listener.0 as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| "TCP listener is closed".to_owned())?;
        let (connection, _) = listener
            .accept()
            .map_err(|error| format!("could not accept TCP connection: {error}"))?;
        let id = TcpConnectionId(self.connections.len() as u32);
        self.connections.push(Some(connection));
        Ok(id)
    }

    pub(crate) fn read(
        &mut self,
        connection: TcpConnectionId,
        maximum: usize,
    ) -> Result<String, String> {
        let connection = self.connection_mut(connection)?;
        let mut bytes = vec![0; maximum];
        let count = connection
            .read(&mut bytes)
            .map_err(|error| format!("could not read TCP connection: {error}"))?;
        String::from_utf8(bytes[..count].to_vec())
            .map_err(|_| "TCP read was not valid UTF-8".to_owned())
    }

    pub(crate) fn write(&mut self, connection: TcpConnectionId, text: &str) -> Result<(), String> {
        self.connection_mut(connection)?
            .write_all(text.as_bytes())
            .map_err(|error| format!("could not write TCP connection: {error}"))
    }

    pub(crate) fn set_timeout(
        &mut self,
        connection: TcpConnectionId,
        milliseconds: u64,
    ) -> Result<(), String> {
        let connection = self.connection_mut(connection)?;
        let timeout = Some(Duration::from_millis(milliseconds));
        connection
            .set_read_timeout(timeout)
            .and_then(|_| connection.set_write_timeout(timeout))
            .map_err(|error| format!("could not set TCP timeout: {error}"))
    }

    pub(crate) fn close_listener(&mut self, listener: TcpListenerId) -> Result<(), String> {
        self.listeners
            .get_mut(listener.0 as usize)
            .ok_or_else(|| "invalid TCP listener".to_owned())?
            .take()
            .map(|_| ())
            .ok_or_else(|| "TCP listener is already closed".to_owned())
    }

    pub(crate) fn close_connection(&mut self, connection: TcpConnectionId) -> Result<(), String> {
        self.connections
            .get_mut(connection.0 as usize)
            .ok_or_else(|| "invalid TCP connection".to_owned())?
            .take()
            .map(|_| ())
            .ok_or_else(|| "TCP connection is already closed".to_owned())
    }

    fn connection_mut(&mut self, connection: TcpConnectionId) -> Result<&mut TcpStream, String> {
        self.connections
            .get_mut(connection.0 as usize)
            .and_then(Option::as_mut)
            .ok_or_else(|| "TCP connection is closed".to_owned())
    }
}
