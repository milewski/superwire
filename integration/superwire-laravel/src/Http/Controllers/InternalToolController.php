<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Http\Controllers;

use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Routing\Controller;
use InvalidArgumentException;
use Superwire\Laravel\Data\ToolInvocationRequest;
use Superwire\Laravel\Data\ToolInvocationResponse;
use Superwire\Laravel\Security\InternalRequestGuard;
use Superwire\Laravel\Support\ToolRegistry;
use Throwable;

final class InternalToolController extends Controller
{
    public function __construct(
        private readonly InternalRequestGuard $internalRequestGuard,
        private readonly ToolRegistry $toolRegistry,
    )
    {
    }

    public function __invoke(Request $request, string $tool): JsonResponse
    {
        $this->internalRequestGuard->assertAuthorized($request);

        try {

            $toolClass = $this->toolRegistry->resolveToolClass($tool);
            $toolInstance = app($toolClass);

            $toolInvocationRequest = $this->toolInvocationRequest($request, $tool);
            $toolInvocationResponse = new ToolInvocationResponse(
                $toolInstance->execute($toolInvocationRequest->agentInput, $toolInvocationRequest->boundInput),
            );

            return response()->json($toolInvocationResponse->output);

        } catch (Throwable $throwable) {

            return response()->json([
                'error' => $throwable->getMessage(),
            ], 500);

        }
    }

    private function toolInvocationRequest(Request $request, string $toolName): ToolInvocationRequest
    {
        $agentInput = $request->input('agent_input', []);
        $boundInput = $request->input('bound_input', []);

        if (!is_array($agentInput) || !is_array($boundInput)) {
            throw new InvalidArgumentException('invalid tool payload');
        }

        return new ToolInvocationRequest($toolName, $agentInput, $boundInput);
    }
}
