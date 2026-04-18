<?php

declare(strict_types=1);

namespace Superwire\Laravel\Driver;

use Closure;
use RuntimeException;
use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\AgentExecutionResult;
use Superwire\Contracts\Contracts\AgentDriverInterface;

final class PrismAgentDriver implements AgentDriverInterface
{
    /**
     * @param Closure(AgentExecutionRequest): AgentExecutionResult|null $executor
     */
    public function __construct(
        private readonly ?Closure $executor = null,
    ) {
    }

    public function execute(AgentExecutionRequest $request): AgentExecutionResult
    {
        if ($this->executor !== null) {
            return ($this->executor)($request);
        }

        return $this->executeViaPrism($request);
    }

    private function executeViaPrism(AgentExecutionRequest $request): AgentExecutionResult
    {
        $prismClassName = 'Prism\\Prism\\Prism';

        if (!class_exists($prismClassName)) {
            throw new RuntimeException('prism driver is not available because prism-php/prism is not installed');
        }

        if (!method_exists($prismClassName, 'text')) {
            throw new RuntimeException('prism driver expects Prism::text() API support');
        }

        $builder = $prismClassName::text();

        if (method_exists($builder, 'usingProvider')) {
            $builder = $builder->usingProvider($request->providerName);
        }

        if (method_exists($builder, 'using')) {
            $builder = $builder->using($request->model);
        }

        if (method_exists($builder, 'withPrompt')) {
            $builder = $builder->withPrompt($request->prompt);
        } elseif (method_exists($builder, 'prompt')) {
            $builder = $builder->prompt($request->prompt);
        }

        if (method_exists($builder, 'withContext') && $request->context !== null) {
            $builder = $builder->withContext($request->context);
        }

        if (method_exists($builder, 'generate')) {
            $response = $builder->generate();
        } elseif (method_exists($builder, 'asText')) {
            $response = $builder->asText();
        } else {
            throw new RuntimeException('prism driver could not find a supported terminal execution method');
        }

        $text = $this->extractText($response);

        return new AgentExecutionResult(
            output: $text,
            context: $text,
            metadata: [
                'driver' => 'prism',
            ]
        );
    }

    private function extractText(mixed $response): string
    {
        if (is_string($response)) {
            return $response;
        }

        if (is_object($response)) {
            if (isset($response->text) && is_string($response->text)) {
                return $response->text;
            }

            if (method_exists($response, 'text')) {
                $textValue = $response->text();

                if (is_string($textValue)) {
                    return $textValue;
                }
            }

            if (method_exists($response, '__toString')) {
                return (string) $response;
            }
        }

        if (is_array($response) && array_key_exists('text', $response) && is_string($response['text'])) {
            return $response['text'];
        }

        throw new RuntimeException('prism driver returned an unsupported response payload');
    }
}
