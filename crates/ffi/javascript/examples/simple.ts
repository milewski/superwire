import { Engine, schema, Tool, type ToolArguments, Workflow } from '../src'

type WeatherInput = {
    region: string;
}

type WeatherBoundedInput = Record<string, never>

type WeatherOutput = {
    prediction: string;
    region_name: string;
    region_label: string;
    latitude: number;
    longitude: number;
    temperature_2m: number;
    weather_code: number;
    weather_description: string;
    wind_speed_10m: number;
}

type WeatherArguments = ToolArguments<WeatherInput, WeatherBoundedInput>

class Weather extends Tool<WeatherInput, WeatherOutput, WeatherBoundedInput> {
    readonly description = 'Get current weather for a specific region'

    readonly inputSchema = schema.object({
        region: schema.string(),
    })

    async execute(toolArguments: WeatherArguments): Promise<WeatherOutput> {
        const regionName = toolArguments.input.region.trim()

        if (!regionName) {
            throw new Error('`region` must be a non-empty string')
        }

        const geocodingResponse = await fetch(
            `https://geocoding-api.open-meteo.com/v1/search?name=${ encodeURIComponent(regionName) }&count=1&language=en&format=json`,
        )

        if (!geocodingResponse.ok) {
            throw new Error(`Geocoding request failed with status ${ geocodingResponse.status }`)
        }

        const geocodingPayload = await geocodingResponse.json() as {
            results?: Array<{
                name: string;
                country?: string;
                admin1?: string;
                latitude: number;
                longitude: number;
            }>;
        }

        const topRegionMatch = geocodingPayload.results?.[ 0 ]

        if (!topRegionMatch) {
            throw new Error(`Could not find region \`${ regionName }\``)
        }

        const weatherResponse = await fetch(
            `https://api.open-meteo.com/v1/forecast?latitude=${ topRegionMatch.latitude }&longitude=${ topRegionMatch.longitude }&current=temperature_2m,weather_code,wind_speed_10m&timezone=auto`,
        )

        if (!weatherResponse.ok) {
            throw new Error(`Weather request failed with status ${ weatherResponse.status }`)
        }

        const weatherPayload = await weatherResponse.json() as {
            current?: {
                temperature_2m?: number;
                weather_code?: number;
                wind_speed_10m?: number;
            };
        }

        const currentWeather = weatherPayload.current

        if (!currentWeather) {
            throw new Error('Weather response did not include current conditions')
        }

        if (typeof currentWeather.temperature_2m !== 'number') {
            throw new Error('Weather response did not include a numeric temperature_2m value')
        }

        if (typeof currentWeather.weather_code !== 'number') {
            throw new Error('Weather response did not include a numeric weather_code value')
        }

        if (typeof currentWeather.wind_speed_10m !== 'number') {
            throw new Error('Weather response did not include a numeric wind_speed_10m value')
        }

        const weatherDescription = this.describeWeatherCode(currentWeather.weather_code)
        const resolvedRegionName = [ topRegionMatch.name, topRegionMatch.admin1, topRegionMatch.country ]
            .filter(Boolean)
            .join(', ')

        return {
            prediction: `${ resolvedRegionName }: ${ weatherDescription }, ${ currentWeather.temperature_2m }°C, wind ${ currentWeather.wind_speed_10m } km/h.`,
            region_name: topRegionMatch.name,
            region_label: resolvedRegionName,
            latitude: topRegionMatch.latitude,
            longitude: topRegionMatch.longitude,
            temperature_2m: currentWeather.temperature_2m,
            weather_code: currentWeather.weather_code,
            weather_description: weatherDescription,
            wind_speed_10m: currentWeather.wind_speed_10m,
        }
    }

    private describeWeatherCode(weatherCode?: number): string {
        const weatherDescriptions = new Map<number, string>([
            [ 0, 'clear sky' ],
            [ 1, 'mainly clear' ],
            [ 2, 'partly cloudy' ],
            [ 3, 'overcast' ],
            [ 45, 'foggy' ],
            [ 48, 'depositing rime fog' ],
            [ 51, 'light drizzle' ],
            [ 53, 'drizzle' ],
            [ 55, 'dense drizzle' ],
            [ 56, 'freezing drizzle' ],
            [ 57, 'dense freezing drizzle' ],
            [ 61, 'slight rain' ],
            [ 63, 'rain' ],
            [ 65, 'heavy rain' ],
            [ 66, 'light freezing rain' ],
            [ 67, 'heavy freezing rain' ],
            [ 71, 'slight snow' ],
            [ 73, 'snow' ],
            [ 75, 'heavy snow' ],
            [ 77, 'snow grains' ],
            [ 80, 'rain showers' ],
            [ 81, 'rain showers' ],
            [ 82, 'violent rain showers' ],
            [ 85, 'snow showers' ],
            [ 86, 'heavy snow showers' ],
            [ 95, 'thunderstorm' ],
            [ 96, 'thunderstorm with hail' ],
            [ 99, 'strong thunderstorm with hail' ],
        ])

        return weatherDescriptions.get(weatherCode ?? -1) ?? 'unknown conditions'
    }
}

async function runSimpleExample(): Promise<void> {
    const workflow = new Workflow(`
        provider openai_local {
            driver: "openai"
            endpoint: "http://169.254.83.107:1234/v1"
            api_key: "local-api-key"
            models: ["qwen3.5-9b"]
        }

        input {
            region: string
        }

        agent assistant {
            model: openai_local("qwen3.5-9b")
            tools: [tool.weather]
            prompt: "Call tool.weather for region '{{ input.region }}' and return the complete flat weather object exactly as provided by the tool, preserving every field."
            output: {
                prediction: string
                region_name: string
                region_label: string
                latitude: number
                longitude: number
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

    const response = await engine.run<Response>(workflow, { region: 'Shanghai' })

    if (await response.isError()) {
        console.error(await response.error())
        engine.close()

        return
    }

    console.log(await response.success())

    engine.close()
}

runSimpleExample()
