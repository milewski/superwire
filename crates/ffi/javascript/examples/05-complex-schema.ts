import { ExecutionEngine } from '../index.js'
import { ZodTool } from '../tool-helper.js'
import { z } from 'zod'

class DatabaseTool extends ZodTool {
    constructor() {
        super(
            'query_database',
            'Query a database with filters',
            z.object({
                table: z.string().describe('Table name'),
                filters: z.object({
                    field: z.string(),
                    operator: z.enum([ 'eq', 'gt', 'lt', 'contains' ]),
                    value: z.union([ z.string(), z.number() ]),
                }).optional().describe('Optional filters'),
                limit: z.number().min(1).max(100).default(10).describe('Maximum number of results'),
            }),
            (params) => {
                return {
                    table: params.table,
                    filters: params.filters,
                    limit: params.limit,
                    results: [
                        { id: 1, name: 'Item 1' },
                        { id: 2, name: 'Item 2' },
                    ],
                }
            },
        )
    }
}

async function main(): Promise<void> {
    console.log('=== Example 5: Complex Schema with Nested Objects ===')

    const dbTool = new DatabaseTool()
    const engine = ExecutionEngine.withTools([ dbTool ])

    const workflow = `
        provider ollama {
            driver <- "ollama"
            models <- ["qwen3:8b"]
            config <- { endpoint <- "http://100.76.5.36:11434" }
        }
        
        agent db_agent {
            model <- "ollama/qwen3:8b"
            tools <- [tool.query_database]
            output <- string
            prompt <- "Query the users table and get the first 5 results"
        }
  `

    try {
        const result = await engine.executeWorkflowContent(workflow)
        console.log(result)
    } catch (error) {
        console.error('Error:', (error as Error).message)
    }
}

main().catch(console.error)
