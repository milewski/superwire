<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support;

use RuntimeException;
use Superwire\Laravel\Data\Workflow\AgentPrompt;
use Superwire\Laravel\Data\Workflow\PromptTemplatePart;

final class PromptParser
{
    /**
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $scope
     */
    public function render(AgentPrompt $prompt, array $agentOutputs, array $scope = []): string
    {
        if ($prompt->isText()) {
            return $prompt->text ?? '';
        }

        $renderedPrompt = '';

        foreach ($prompt->templateParts as $templatePart) {
            $renderedPrompt .= $this->renderTemplatePart($templatePart, $agentOutputs, $scope);
        }

        return $renderedPrompt;
    }

    /**
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $scope
     */
    private function renderTemplatePart(PromptTemplatePart $templatePart, array $agentOutputs, array $scope): string
    {
        if ($templatePart->isText()) {
            return $templatePart->text ?? '';
        }

        if (!$templatePart->isExpression() || $templatePart->expression === null) {
            throw new RuntimeException('Prompt template part must contain text or an expression.');
        }

        return (string)$this->resolveReference($templatePart->expression->reference, $agentOutputs, $scope);
    }

    /**
     * @param array<string, mixed> $agentOutputs
     * @param array<string, mixed> $scope
     */
    public function resolveReference(string $reference, array $agentOutputs, array $scope = []): mixed
    {
        if (array_key_exists($reference, $scope)) {
            return $scope[ $reference ];
        }

        $segments = explode('.', $reference);

        if ($segments[ 0 ] !== 'agent') {
            throw new RuntimeException(sprintf('Unsupported reference: %s', $reference));
        }

        if (count($segments) < 2) {
            throw new RuntimeException(sprintf('Invalid agent reference: %s', $reference));
        }

        $agentName = $segments[ 1 ];

        if (!array_key_exists($agentName, $agentOutputs)) {
            throw new RuntimeException(sprintf('Referenced agent output is not available: %s', $reference));
        }

        $resolvedValue = $agentOutputs[ $agentName ];

        foreach (array_slice($segments, 2) as $segment) {
            if (!is_array($resolvedValue) || !array_key_exists($segment, $resolvedValue)) {
                throw new RuntimeException(sprintf('Reference segment could not be resolved: %s', $reference));
            }

            $resolvedValue = $resolvedValue[ $segment ];
        }

        return $resolvedValue;
    }
}
