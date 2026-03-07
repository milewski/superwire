provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://100.76.5.36:11434"
    models <- ["qwen3.5:27b"]
}

schema person {
    name: string
    age: number
    hobbies: [string]
}

<- agent extract_info {
    model <- "ollama1/qwen3.5:27b"
    output <- schema.person
    prompt <- """
        Extract information about a person from the following text:

        John is 30 years old and enjoys reading, hiking, and photography.

        Return the information in JSON format.
    """
}

<- agent summary {
    model <- "ollama1/qwen3.5:27b"

    prompt <- """
        Create a brief summary about {{ extract_info.name }} who is {{ extract_info.age }} years old.
        hobbies: {{ extract_info.hobbies }}
        call the done tool once you finish your evaluation
    """
}
