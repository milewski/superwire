```wire
// provider declaration
provider openai {
// provider driver
    driver:"openai" // inline driver comment
}



// This file shows the smallest useful workflow shape:
// provider -> agent -> output



// Provider defines where models come from.
provider ollama {
    // inline models comment
    driver: "ollama"
    
    // inline models comment
    models: ["qwen3.5:32b"]
    // inline models comment
    output: string
}

// output heading
output { value: "ok" }
```
---
```wire
// provider declaration
provider openai {
    // provider driver
    driver: "openai" // inline driver comment
}

// This file shows the smallest useful workflow shape:
// provider -> agent -> output

// Provider defines where models come from.
provider ollama {
    // inline models comment
    driver: "ollama"

    // inline models comment
    models: ["qwen3.5:32b"]

    // inline models comment
    output: string
}

// output heading
output {
    value: "ok"
}
```
