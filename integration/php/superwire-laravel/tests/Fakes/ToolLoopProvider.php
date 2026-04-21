<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Fakes;

use Prism\Prism\Providers\Provider;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\Tool;
use RuntimeException;

final class ToolLoopProvider extends Provider
{
    /**
     * @param array<string, mixed> $resultsByPrompt
     */
    public function __construct(
        private readonly array $resultsByPrompt,
    )
    {
    }

    public function text(TextRequest $request): never
    {
        $prompt = $request->prompt();

        if ($prompt === null || !array_key_exists($prompt, $this->resultsByPrompt)) {
            throw new RuntimeException(sprintf('No fake tool-loop response registered for prompt: %s', $prompt ?? 'null'));
        }

        $result = $this->resultsByPrompt[ $prompt ];
        $finalizeTool = $this->resolveTool('finalize_success', $request->tools());

        $finalizeTool->handle(...[ 'result' => $result ]);

        throw new RuntimeException('Finalize tool did not terminate execution.');
    }

    /**
     * @param array<int, Tool> $tools
     */
    private function resolveTool(string $name, array $tools): Tool
    {
        foreach ($tools as $tool) {
            if ($tool->name() === $name) {
                return $tool;
            }
        }

        throw new RuntimeException(sprintf('Tool not found in fake provider: %s', $name));
    }
}
