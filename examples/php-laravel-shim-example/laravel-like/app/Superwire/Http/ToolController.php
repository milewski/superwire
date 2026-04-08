<?php

namespace App\Superwire\Http;

use App\Superwire\Security\InternalRequestGuard;
use App\Superwire\Tool;
use App\Superwire\Tools\WeatherTool;
use RuntimeException;

final class ToolController
{
    public function execute(string $toolName, array $server, string $requestBody): array
    {
        $requestPayload = json_decode($requestBody, true);

        if (!is_array($requestPayload)) {
            throw new RuntimeException('invalid_request_payload');
        }

        $agentInput = $requestPayload['agent_input'] ?? [];
        $boundInput = $requestPayload['bound_input'] ?? [];

        if (!is_array($agentInput) || !is_array($boundInput)) {
            throw new RuntimeException('invalid_tool_input_shape');
        }

        $boundInputToken = isset($boundInput['internal_token']) ? (string) $boundInput['internal_token'] : null;

        InternalRequestGuard::assertAuthorized($server, $boundInputToken);

        $tool = $this->resolveTool($toolName);

        return $tool->execute($agentInput, $boundInput);
    }

    private function resolveTool(string $toolName): Tool
    {
        return match ($toolName) {
            'weather' => new WeatherTool(),
            default => throw new RuntimeException('unknown_tool'),
        };
    }
}
