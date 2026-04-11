<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution\Compiler;

use Superwire\Laravel\Exceptions\ToolBuildException;

final readonly class ToolModuleTemplateRenderer
{
    public function __construct(private string $templatePath)
    {
    }

    /**
     * @param array<string, string> $templateVariables
     */
    public function render(array $templateVariables): string
    {
        if (!is_file($this->templatePath)) {
            throw new ToolBuildException(sprintf('tool module template not found at %s', $this->templatePath));
        }

        $templateSource = file_get_contents($this->templatePath);

        if ($templateSource === false) {
            throw new ToolBuildException(sprintf('failed to read tool module template at %s', $this->templatePath));
        }

        $placeholderValues = [];

        foreach ($templateVariables as $templateVariableName => $templateVariableValue) {
            $placeholderValues[ sprintf('{{%s}}', $templateVariableName) ] = $templateVariableValue;
        }

        $renderedTemplate = strtr($templateSource, $placeholderValues);

        if (preg_match('/\{\{[a-z0-9_]+\}\}/', $renderedTemplate) === 1) {
            throw new ToolBuildException('tool module template rendering failed because unresolved placeholders remain');
        }

        return $renderedTemplate;
    }
}
