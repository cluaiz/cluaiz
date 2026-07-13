use dispatcher::NeuralDispatcher;

/// Shared application state containing the dispatcher.
pub struct AppState {
    pub dispatcher: NeuralDispatcher,
    pub embedding_dispatcher: std::sync::Arc<dispatcher::EmbeddingDispatcher>,
}
