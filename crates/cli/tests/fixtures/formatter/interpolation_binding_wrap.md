```wire
input { product_name: string release_highlights: [string] }

agent release_email {
    model: ollama("qwen3.5:8b")
    prompt: "Write a customer announcement email for {{ input.product_name }} with these highlights: {{ input.release_highlights }} and keep the tone warm and concise for existing customers."
    output: string
}

agent customer_email {
    model: openai("gpt-4.1-mini")
    prompt: "Write a customer announcement email for {{ input.product_name }} with these highlights: {{ input.release_highlights }}"
    output: {
        subject: string
        body: string
    }
}
```
---
```wire
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

agent customer_email {
    model: openai("gpt-4.1-mini")
    prompt: "Write a customer announcement email for {{ input.product_name }} with these highlights: {{ input.release_highlights }}"
    output: {
        subject: string
        body: string
    }
}
```
