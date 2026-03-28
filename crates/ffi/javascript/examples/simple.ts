import { Engine, schema, Tool, type ToolArguments, Workflow } from '../src'
import { loadOpenAIProviderSecrets } from './env'

type WeatherInput = {
    region: string;
}

type WeatherBoundedInput = Record<string, never>

type WeatherOutput = {
    region_name: string;
    region_label: string;
    temperature_2m: number;
    weather_code: number;
    weather_description: string;
    wind_speed_10m: number;
    location: {
        latitude: number;
        longitude: number;
    }
}

type WeatherArguments = ToolArguments<WeatherInput, WeatherBoundedInput>

class Weather extends Tool<WeatherInput, WeatherOutput, WeatherBoundedInput> {
    readonly description = 'Get current weather for a specific region'

    readonly inputSchema = schema.object({
        region: schema.string(),
    })

    async execute(toolArguments: WeatherArguments): Promise<WeatherOutput> {
        const regionName = toolArguments.input.region.trim()
        const weatherResponse = await fetch(`https://wttr.ixn/${ encodeURIComponent(regionName) }?format=j1`)

        if (!weatherResponse.ok) {
            throw new Error(`Weather request failed with status ${ weatherResponse.status }`)
        }

        const weatherPayload = await weatherResponse.json() as {
            current_condition?: Array<{
                temp_C?: string;
                weatherCode?: string;
                weatherDesc?: Array<{ value?: string }>;
                windspeedKmph?: string;
            }>;
            nearest_area?: Array<{
                areaName?: Array<{ value?: string }>;
                region?: Array<{ value?: string }>;
                country?: Array<{ value?: string }>;
                latitude?: string;
                longitude?: string;
            }>;
        }

        const currentWeather = weatherPayload.current_condition?.[ 0 ]

        if (!currentWeather) {
            throw new Error('Weather response did not include current conditions')
        }

        const nearestArea = weatherPayload.nearest_area?.[ 0 ]
        const resolvedRegionName = [
            nearestArea?.areaName?.[ 0 ]?.value,
            nearestArea?.region?.[ 0 ]?.value,
            nearestArea?.country?.[ 0 ]?.value,
        ]
            .filter(Boolean)
            .join(', ') || regionName

        const regionDisplayName = nearestArea?.areaName?.[ 0 ]?.value ?? regionName
        const weatherDescription = currentWeather.weatherDesc?.[ 0 ]?.value ?? 'unknown conditions'
        const temperatureInCelsius = Number(currentWeather.temp_C)
        const weatherCode = Number(currentWeather.weatherCode)
        const windSpeedInKmph = Number(currentWeather.windspeedKmph)
        const latitude = Number(nearestArea?.latitude)
        const longitude = Number(nearestArea?.longitude)

        return {
            region_name: regionDisplayName,
            region_label: resolvedRegionName,
            temperature_2m: temperatureInCelsius,
            weather_code: weatherCode,
            weather_description: weatherDescription,
            wind_speed_10m: windSpeedInKmph,
            location: {
                latitude,
                longitude,
            },
        }
    }
}

async function runSimpleExample(): Promise<void> {
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
            region: string
        }

        agent assistant {
            model: openai(secrets.openai_model)
            tools: [tool.weather]
            prompt: "please get the weather for the given city: {{ input.region }} and generate a prediction based on the metrics provided by the weather tool."
            output: {
                prediction: string
                region_name: string
                region_label: string
                location: {
                    latitude: float
                    longitude: float
                }
                temperature_2m: number
                weather_code: number
                weather_description: string
                wind_speed_10m: number
            }
        }

        output {
            weather: agent.assistant
        }
    `)

    type Response = {
        weather: WeatherOutput;
    }

    const engine = new Engine()

    engine.registerTool(new Weather())

    const response = await engine.run<Response>(workflow, { region: 'Shanghai' }, providerSecrets)
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
