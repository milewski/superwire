TODO

- Create an example fixture demonstrating that {{ secrets.xxx }} cannot be used within prompts, trying to use should throw an error, the reason being that this would leak the secret to the context of the model

---

instead of having 2 endpoints 1 for json and another for stream can have a single endpoint and the accept: application/json or event/stream defines if user wants a json or a streamable response i think this is cleaner 

---- 

- Update all the `output: value` from agent blocks to `output { xxx:xxxx }` the raw output: string is no longer supported, so please update everything, all tests, fixtures, remove support completely from the DSL, AST for defining output of an agent using this type of definition so 

agent {
  output {
     /// this is the correct form
     correct: string
  }
}

agent {
    output string // this is invalid form now on
}

do not need to make it retro compatible, completely remove the option to allow to define output like this, ensure all tests passes at the end with cargo test

---