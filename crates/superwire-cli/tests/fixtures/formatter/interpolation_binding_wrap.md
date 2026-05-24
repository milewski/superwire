```wire
input { product_name: string release_highlights: [string] }

agent release_email {
    model: model.ollama_model
    instruction: "Write a customer announcement email for {{ input.product_name }} with these highlights: {{ input.release_highlights }} and keep the tone warm and concise for existing customers."
    output {
        value: string
    }
}

agent customer_email {
    model: model.openai_model
    instruction: "Write a customer announcement email for {{ input.product_name }} with these highlights: {{ input.release_highlights }}"
    output {
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
    model: model.ollama_model

    instruction: """
        Write a customer announcement email for {{ input.product_name }} with these highlights:
        {{ input.release_highlights }} and keep the tone warm and concise for existing customers.
    """

    output {
        value: string
    }
}

agent customer_email {
    model: model.openai_model
    instruction: "Write a customer announcement email for {{ input.product_name }} with these highlights: {{ input.release_highlights }}"
    output {
        subject: string
        body: string
    }
}
```
