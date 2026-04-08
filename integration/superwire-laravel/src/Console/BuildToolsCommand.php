<?php

namespace Superwire\Laravel\Console;

use Illuminate\Console\Command;
use Superwire\Laravel\Data\ToolBuildRequest;
use Superwire\Laravel\Execution\ToolCompiler;

final class BuildToolsCommand extends Command
{
    protected $signature = 'superwire:tools:build {--tool=* : Fully-qualified PHP tool classes to compile into wasm}';

    protected $description = 'Compile PHP tool proxies into WASM modules for workflow runtime';

    public function __construct(private readonly ToolCompiler $toolCompiler)
    {
        parent::__construct();
    }

    public function handle(): int
    {
        $toolClasses = $this->option('tool');

        if (!is_array($toolClasses) || empty($toolClasses)) {
            $configuredToolClasses = config('superwire.tools.registered_classes', []);
            $toolClasses = is_array($configuredToolClasses) ? $configuredToolClasses : [];
        }

        if (empty($toolClasses)) {
            $this->error('No tool classes provided. Use --tool=App\\Superwire\\Tools\\WeatherTool or configure superwire.tools.registered_classes.');

            return self::FAILURE;
        }

        $buildResult = $this->toolCompiler->build(new ToolBuildRequest($toolClasses));

        $this->info(sprintf('Built %d tool module(s) into %s', count($buildResult->toolNames), $buildResult->outputDirectory));

        foreach ($buildResult->toolNames as $toolName) {
            $this->line(sprintf('- %s.wasm', $toolName));
        }

        return self::SUCCESS;
    }
}
