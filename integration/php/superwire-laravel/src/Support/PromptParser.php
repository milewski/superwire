<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support;

use RuntimeException;
use Superwire\Laravel\AgentExecutionResult;
use Superwire\Laravel\Data\Prompt\Prompt;
use Superwire\Laravel\Data\Prompt\PromptTemplatePart;

final class PromptParser
{
    /**
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $scope
     * @param array<string, mixed> $inputValues
     * @param array<string, mixed> $secretValues
     */
    public function render(Prompt $prompt, array $agentOutputs, array $scope = [], array $inputValues = [], array $secretValues = []): string
    {
        if ($prompt->isText()) {
            return $prompt->text ?? '';
        }

        $renderedPrompt = '';

        foreach ($prompt->templateParts as $templatePart) {
            $renderedPrompt .= $this->renderTemplatePart($templatePart, $agentOutputs, $scope, $inputValues, $secretValues);
        }

        return $renderedPrompt;
    }

    /**
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $scope
     * @param array<string, mixed> $inputValues
     * @param array<string, mixed> $secretValues
     */
    private function renderTemplatePart(PromptTemplatePart $templatePart, array $agentOutputs, array $scope, array $inputValues, array $secretValues): string
    {
        if ($templatePart->isText()) {
            return $templatePart->text ?? '';
        }

        if (!$templatePart->isExpression() || $templatePart->expression === null) {
            throw new RuntimeException('Prompt template part must contain text or an expression.');
        }

        $resolvedValue = $this->resolveReference($templatePart->expression->reference, $agentOutputs, $scope, $inputValues, $secretValues);

        if (is_array($resolvedValue)) {
            return json_encode($resolvedValue, JSON_THROW_ON_ERROR);
        }

        if (is_bool($resolvedValue)) {
            return $resolvedValue ? 'true' : 'false';
        }

        if ($resolvedValue === null) {
            return 'null';
        }

        return (string) $resolvedValue;
    }

    /**
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $scope
     * @param array<string, mixed> $inputValues
     * @param array<string, mixed> $secretValues
     */
    public function resolveReference(string $reference, array $agentOutputs, array $scope = [], array $inputValues = [], array $secretValues = []): mixed
    {
        if (array_key_exists($reference, $scope)) {
            return $scope[ $reference ];
        }

        $segments = explode('.', $reference);
        $referenceRoot = $segments[ 0 ] ?? null;

        if (!is_string($referenceRoot)) {
            throw new RuntimeException(sprintf('Unsupported reference: %s', $reference));
        }

        if (array_key_exists($referenceRoot, $scope)) {
            return $this->resolveSegments($reference, $scope[ $referenceRoot ], array_slice($segments, 1));
        }

        $rootValue = match ($referenceRoot) {
            'agent' => $this->resolveAgentRoot($reference, $segments, $agentOutputs),
            'input' => $inputValues,
            'secrets' => $secretValues,
            default => throw new RuntimeException(sprintf('Unsupported reference: %s', $reference)),
        };

        return $this->resolveSegments(
            $reference,
            $rootValue,
            $referenceRoot === 'agent' ? array_slice($segments, 2) : array_slice($segments, 1),
        );
    }

    /**
     * @param array<int, string> $segments
     * @param array<string, mixed> $agentOutputs
     */
    private function resolveAgentRoot(string $reference, array $segments, array $agentOutputs): mixed
    {
        if (count($segments) < 2) {
            throw new RuntimeException(sprintf('Invalid agent reference: %s', $reference));
        }

        $agentName = $segments[ 1 ];

        if (!array_key_exists($agentName, $agentOutputs)) {
            throw new RuntimeException(sprintf('Referenced agent output is not available: %s', $reference));
        }

        $resolvedValue = $agentOutputs[ $agentName ];

        if ($resolvedValue instanceof AgentExecutionResult) {
            return $resolvedValue->output;
        }

        return $resolvedValue;
    }

    /**
     * @param array<int, string> $segments
     */
    private function resolveSegments(string $reference, mixed $resolvedValue, array $segments): mixed
    {
        foreach ($segments as $segmentIndex => $segment) {

            if ($segmentIndex === 0 && $segment === 'agent') {
                continue;
            }

            if (!is_array($resolvedValue) || !array_key_exists($segment, $resolvedValue)) {
                throw new RuntimeException(sprintf('Reference segment could not be resolved: %s', $reference));
            }

            $resolvedValue = $resolvedValue[ $segment ];

        }

        return $resolvedValue;
    }
}
