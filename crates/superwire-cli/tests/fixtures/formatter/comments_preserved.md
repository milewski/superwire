```wire
// provider declaration
provider openai from openai {
// provider driver
}



// This file shows the smallest useful workflow shape:
// provider -> agent -> output



// Provider defines where models come from.
provider ollama from ollama {
    // inline models comment
}

model ollama_model from ollama {
    id: "qwen3.5:32b"
}

// output heading
output { value: "ok" }
```
---
```wire
// provider declaration
provider openai from openai {
// provider driver
}

// This file shows the smallest useful workflow shape:
// provider -> agent -> output

// Provider defines where models come from.
provider ollama from ollama {
// inline models comment
}

model ollama_model from ollama {
    id: "qwen3.5:32b"
}

// output heading
output {
    value: "ok"
}
```
