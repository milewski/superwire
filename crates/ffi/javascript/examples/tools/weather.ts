import { schema, Tool, type ToolArguments } from '../../src'

export type WeatherInput = {
    region: string;
}

type WeatherBoundedInput = Record<string, never>

export type WeatherOutput = {
    user_id: string;
    price_range: string;
}

type WeatherArguments = ToolArguments<WeatherInput, WeatherBoundedInput>

export class Database extends Tool<WeatherInput, WeatherOutput, WeatherBoundedInput> {
    readonly description = 'Get current weather for a specific region'

    async execute(toolArguments: WeatherArguments): Promise<WeatherOutput> {

       const response = databaseService.findByUserId(toolArguments.input.user_id)
        
        return {
            test: "abc"
        }
    }
}
