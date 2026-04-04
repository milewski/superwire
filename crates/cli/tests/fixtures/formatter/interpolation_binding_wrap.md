```ai
input { product_name: string release_highlights: [string] }

agent release_email {
    model: ollama("qwen3.5:8b")
    prompt: "Write a customer announcement email for {{ input.product_name }} with these highlights: {{ input.release_highlights }} and keep the tone warm and concise for existing customers."
    output: string
}
```
---
```ai
input {
    product_name: string
    release_highlights: [string]
}

agent release_email {
    model: ollama("qwen3.5:8b")

    prompt: """
        Write a customer announcement email for {{ input.product_name }} with these highlights:
        {{ input.release_highlights }} and keep the tone warm and concise for existing customers.
    """

    output: string
}
```
