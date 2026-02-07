//! TreeRAG Context Management
//!
//! Provides intelligent context management for AI agents using
//! hybrid retrieval with tree-based and semantic search.

mod error;
mod manager;
mod memory;
mod render;
mod router;
mod scope;

pub use error::ContextError;
pub use manager::{ContextManager, ScopeRequest};
pub use memory::{MemoryStore, MemoryStoreError, MemorySyncStats};
pub use render::ContextRenderer;
pub use router::{HybridRouter, QueryIntent, RetrievalResult};
pub use scope::{AnchorContext, ContextScope, Experience, FocusContext, HorizonContext, Outcome};
