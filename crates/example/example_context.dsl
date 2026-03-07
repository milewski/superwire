provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://100.76.5.36:11434"
    models <- ["qwen3.5:27b"]
}

agent discuss_topic {
    model <- "ollama1/qwen3.5:27b"
    prompt <- """
        Let's discuss artificial intelligence and its impact on society.
        What are the main benefits?
    """
}

agent continue_discussion {
    model <- "ollama1/qwen3.5:27b"
    context <- agent.discuss_topic.context
    prompt <- """
        That's interesting. Now what about the potential risks?
    """
}

<- agent summarize {
    model <- "ollama1/qwen3.5:27b"
    context <- agent.continue_discussion.context.summary
    prompt <- """
        Based on our discussion, provide a balanced summary of
        AI's benefits and risks in 2-3 sentences.
    """
}
