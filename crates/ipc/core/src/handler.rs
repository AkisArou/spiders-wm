use spiders_core::command::WmCommand;
use spiders_core::query::{QueryRequest, QueryResponse};

use crate::{DebugRequest, DebugResponse};

pub trait IpcHandler {
    type Error;

    fn handle_query(&mut self, query: QueryRequest) -> Result<QueryResponse, Self::Error>;
    fn handle_command(&mut self, command: WmCommand) -> Result<(), Self::Error>;
    fn handle_debug(&mut self, request: DebugRequest) -> Result<DebugResponse, Self::Error>;
}
