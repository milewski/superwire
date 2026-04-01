<?php

declare(strict_types = 1);

use EngineAi\Ffi\EngineFfiBridge;
use EngineAi\Ffi\ExecutionValueName;

require __DIR__ . '/../vendor/autoload.php';

$workflowSource = (string) file_get_contents(__DIR__ . '/workflows/direct_bridge.ai');

$bridge = new EngineFfiBridge();

$workflowExecutionEnvelope = $bridge->executeWorkflow([
    'execution_id' => 'direct-bridge-example',
    'workflow_source' => $workflowSource,
    'input' => [
        'payload' => [
            'department' => 'platform',
            'active_incidents' => 2,
        ],
    ],
    'custom_tools' => [],
    'defer_output' => true,
], [
    'requestId' => 'example-request-1',
]);

echo "Workflow execute response:\n";
print_r($workflowExecutionEnvelope);

$readExecutionEnvelope = $bridge->readExecutionValue([
    'execution_id' => 'direct-bridge-example',
    'value' => ExecutionValueName::SUCCESS,
]);

echo "Read deferred execution value response:\n";
print_r($readExecutionEnvelope);

$bridge->close();
