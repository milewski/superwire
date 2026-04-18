<?php

declare(strict_types=1);

namespace Superwire\Laravel\Support;

use Illuminate\Support\Facades\Process;
use RuntimeException;
use Superwire\Contracts\Support\JsonWorkflowDecoder;
use Superwire\Contracts\WorkflowDefinition;

final class CachedWorkflowDefinitionCompiler
{
    /**
     * @var array<string, WorkflowDefinition>
     */
    private static array $definitionsByCacheKey = [];

    public function compile(string $workflowPath): WorkflowDefinition
    {
        $absolutePath = str_starts_with($workflowPath, '/') ? $workflowPath : base_path($workflowPath);

        if (!is_file($absolutePath)) {
            throw new RuntimeException("workflow file `{$absolutePath}` was not found");
        }

        $cacheKey = $this->cacheKey($absolutePath);

        if (array_key_exists($cacheKey, self::$definitionsByCacheKey)) {
            return self::$definitionsByCacheKey[$cacheKey];
        }

        $process = Process::path(base_path())
            ->timeout((int) config('superwire.cli.timeout_seconds', 600))
            ->run([
                (string) config('superwire.cli.binary', './superwire-cli'),
                'workflow',
                'to-json',
                $absolutePath,
                '--compact',
            ]);

        if (!$process->successful()) {
            throw new RuntimeException('failed to compile workflow to json: ' . $process->errorOutput());
        }

        $definition = (new JsonWorkflowDecoder())->decodeFromJson($process->output());
        self::$definitionsByCacheKey[$cacheKey] = $definition;

        return $definition;
    }

    private function cacheKey(string $absolutePath): string
    {
        $fileModifiedAt = filemtime($absolutePath);

        return $absolutePath . '::' . ($fileModifiedAt === false ? '0' : (string) $fileModifiedAt);
    }
}
