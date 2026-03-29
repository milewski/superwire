/**
 * Basic workflow + tool callback example.
 *
 * Shows how to:
 * - load a workflow from a `.ai` file
 * - pass `inputs` and `secrets` at workflow creation
 * - attach a workflow-scoped runtime tool (`Weather`) via `tools: []`
 */
import { Engine, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'
import { Weather, type WeatherOutput } from './tools'

async function runSimpleExample(): Promise<void> {
    const providerSecrets = loadOpenAIProviderSecrets()

    const workflow = Workflow.fromFile('./examples/workflows/simple.ai', {
        inputs: {
            region: 'Shanghai',
        },
        secrets: providerSecrets,
        tools: [
            new Weather(),
        ],
    })

    type Response = {
        weather: WeatherOutput;
    }

    const engine = new Engine()

    const response = await engine.run<Response>(workflow)
    const hasError = await response.isError()

    console.log('isError:', hasError)

    if (hasError) {
        const executionError = await response.error()

        console.error('Error details:')
        console.error('  code:', executionError?.code)
        console.error('  message:', executionError?.message)
        console.error('  context:', executionError?.context)
        engine.close()

        return
    }

    // console.log(await response.success())

    engine.close()
}

runSimpleExample()
