<?php

namespace Superwire\Laravel\Console;

use Illuminate\Console\Command;
use Superwire\Laravel\Support\Workflow;

final class RunWorkflowCommand extends Command
{
    protected $signature = 'superwire:workflow:run
        {workflow : Path to workflow file}
        {--input=* : Input fields in key=value format}
        {--secret=* : Secrets fields in key=value format}
        {--tool=* : Fully-qualified PHP tool classes to build before run}';

    protected $description = 'Run a Superwire workflow from Laravel using the configured CLI runtime';

    public function handle(): int
    {
        $workflowFilePath = (string) $this->argument('workflow');
        $inputFields = $this->inputFields((array) $this->option('input'));
        $secretFields = $this->inputFields((array) $this->option('secret'));
        $toolClasses = (array) $this->option('tool');

        $workflow = Workflow::fromFile($workflowFilePath)
            ->withInputs($inputFields)
            ->withSecrets($secretFields);

        if (!empty($toolClasses)) {
            $workflow = $workflow->withTools($toolClasses);
        }

        $workflowOutput = $workflow->run();

        $this->line(json_encode($workflowOutput, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR));

        return self::SUCCESS;
    }

    /**
     * @param list<string> $inputFlags
     * @return array<string, mixed>
     */
    private function inputFields(array $inputFlags): array
    {
        $inputFields = [];

        foreach ($inputFlags as $inputFlag) {
            if (!is_string($inputFlag)) {
                continue;
            }

            [$inputKey, $inputValue] = array_pad(explode('=', $inputFlag, 2), 2, '');

            if ($inputKey === '') {
                continue;
            }

            $inputFields[$inputKey] = $inputValue;
        }

        return $inputFields;
    }
}
