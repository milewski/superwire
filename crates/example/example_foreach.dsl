provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen2.5:3b"]
}

schema hobby_info {
    hobby: string
    difficulty: string
    time_required: string
}

schema person {
    name: string
    age: number
    hobbies: [string]
}

agent extract_person {
    model <- "ollama1/qwen2.5:3b"
    output <- schema.person
    prompt <- """
        Extract information about a person from the following text:

        John is 30 years old and enjoys reading, hiking, and photography.

        Return the information in JSON format with fields: name, age, hobbies (array).
    """
}

agent analyze_hobbies {
    for_each <- extract_person.hobbies as hobby

    model <- "ollama1/qwen2.5:3b"
    output <- schema.hobby_info

    prompt <- """
        Analyze the hobby: {{ hobby }}

        Provide:
        - hobby: the hobby name
        - difficulty: beginner/intermediate/advanced
        - time_required: how much time typically needed

        Return as JSON.
    """
}

<- agent final_report {
    model <- "ollama1/qwen2.5:3b"
    prompt <- """
        Create a comprehensive report about {{ extract_person.name }}.

        Person details:
        - Name: {{ extract_person.name }}
        - Age: {{ extract_person.age }}
        - Number of hobbies: {{ extract_person.hobbies }}

        The person has diverse interests. Write a 2-3 sentence summary
        highlighting their personality based on their hobbies.
    """
}
