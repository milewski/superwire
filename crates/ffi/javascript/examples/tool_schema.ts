import { Engine, schema, Tool, type ToolArguments, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type LaunchTaskToolInput = {
    stage: 'draft' | 'ready' | 'published';
    highlights: string[];
    score_window: [ number, number ];
    owner: string | null;
    priority_filter: 'all' | 'p0' | 'p1' | null;
    include_archived: false;
    top_tags: [ string, string, string ];
}

type LaunchTaskToolBoundedInput = Record<string, never>

type LaunchTaskToolOutput = {
    summary: string;
    recommended_tasks: Array<{
        title: string;
        priority: 'p0' | 'p1' | 'p2';
    }>;
}

type LaunchTaskToolArguments = ToolArguments<LaunchTaskToolInput, LaunchTaskToolBoundedInput>

class LaunchTaskPrioritizer extends Tool<LaunchTaskToolInput, LaunchTaskToolOutput, LaunchTaskToolBoundedInput> {
    readonly description = 'Build a launch task shortlist from release context'

    readonly inputSchema = schema.object({
        stage: schema.enumeration([ 'draft', 'ready', 'published' ]),
        highlights: schema.array(schema.string(), { minItems: 1 }),
        score_window: schema.tuple([ schema.integer(), schema.integer() ]),
        owner: schema.nullable(schema.string()),
        priority_filter: schema.nullable(schema.enumeration([ 'all', 'p0', 'p1' ])),
        include_archived: schema.literal(false),
        top_tags: schema.fixedArray(schema.string(), 3),
    })

    readonly outputSchema = schema.object({
        summary: schema.string(),
        recommended_tasks: schema.array(schema.object({
            title: schema.string(),
            priority: schema.enumeration([ 'p0', 'p1', 'p2' ]),
        })),
    })

    async execute(toolArguments: LaunchTaskToolArguments): Promise<LaunchTaskToolOutput> {
        const releaseStage = toolArguments.input.stage
        const releaseHighlights = toolArguments.input.highlights
        const [ minimumScore, maximumScore ] = toolArguments.input.score_window
        const owner = toolArguments.input.owner
        const priorityFilter = toolArguments.input.priority_filter
        const topTags = toolArguments.input.top_tags

        const computedPriority: 'p0' | 'p1' | 'p2' = priorityFilter === 'p0' || releaseStage === 'published' ? 'p0' : 'p1'

        return {
            summary: `Stage=${ releaseStage }, highlights=${ releaseHighlights.length }, score_window=${ minimumScore }-${ maximumScore }, owner=${ owner ?? 'unassigned' }, tags=${ topTags.join(',') }`,
            recommended_tasks: [
                {
                    title: 'Finalize release notes and changelog',
                    priority: computedPriority,
                },
                {
                    title: 'Validate telemetry and alert coverage',
                    priority: 'p1',
                },
            ],
        }
    }
}

type ToolSchemaInput = {
    product_name: string;
    release_stage: 'draft' | 'ready' | 'published';
    release_highlights: string[];
}

type ToolSchemaResponse = {
    planning: {
        summary: string;
        recommended_tasks: Array<{
            title: string;
            priority: 'p0' | 'p1' | 'p2';
        }>;
    };
}

async function runToolSchemaExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const workflow = new Workflow(`
        provider openai {
            driver: "openai"
            endpoint: secrets.openai_endpoint
            api_key: secrets.openai_api_key
            models: [secrets.openai_model]
        }

        secrets {
            openai_endpoint: string
            openai_api_key: string
            openai_model: string
        }

        input {
            product_name: string
            release_stage: "draft" | "ready" | "published"
            release_highlights: [string]
        }

        agent planner {
            model: openai(secrets.openai_model)
            tools: [
                tool.launch_task_prioritizer(
                    stage: input.release_stage,
                    highlights: input.release_highlights,
                    score_window: [1, 10],
                    owner: null,
                    priority_filter: "all",
                    include_archived: false,
                    top_tags: ["launch", "quality", "docs"]
                )
            ]
            prompt: "Use the tool to produce a launch planning recommendation for {{ input.product_name }}."
            output: {
                summary: string
                recommended_tasks: [{
                    title: string
                    priority: "p0" | "p1" | "p2"
                }]
            }
        }

        output {
            planning: agent.planner
        }
    `)

    const engine = new Engine()

    engine.registerTool(new LaunchTaskPrioritizer())

    try {
        const inputPayload: ToolSchemaInput = {
            product_name: 'Compass AI',
            release_stage: 'ready',
            release_highlights: [
                'one-click setup',
                'new team metrics dashboard',
                'higher multilingual quality',
            ],
        }

        const response = await engine.run<ToolSchemaResponse>(workflow, inputPayload, providerSecrets)

        if (await response.isError()) {
            console.error('Error:', await response.error())

            return
        }

        console.log(await response.success())
    } finally {
        engine.close()
    }
}

runToolSchemaExample()
