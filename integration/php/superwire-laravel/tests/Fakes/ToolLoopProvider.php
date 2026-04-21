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
     * @var array<int, TextRequest>
     */
    private array $requests = [];

    /**
     * @var array<int, array<string, mixed>>
     */
    private array $providerConfigs = [];

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
        $this->requests[] = $request;

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
     * @param array<string, mixed> $providerConfig
     */
    public function recordProviderConfig(array $providerConfig): void
    {
        $this->providerConfigs[] = $providerConfig;
    }

    /**
     * @return array<int, TextRequest>
     */
    public function requests(): array
    {
        return $this->requests;
    }

    /**
     * @return array<int, array<string, mixed>>
     */
    public function providerConfigs(): array
    {
        return $this->providerConfigs;
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
