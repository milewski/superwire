```wire
schema Review {status:"approved"|"rejected" tags:[string;3] pair:(string,number) metadata:{score:number reason:string|   null} }

output { schema_name:"Review" }
```
---
```wire
schema Review {
    status: "approved" | "rejected"
    tags: [string; 3]
    pair: (string, number)
    metadata: {
        score: number
        reason: string | null
    }
}

output {
    schema_name: "Review"
}
```
