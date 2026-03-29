<?php

declare(strict_types=1);

use EngineAi\Ffi\Engine;
use EngineAi\Ffi\Workflow;

require __DIR__ . '/../vendor/autoload.php';

$engine = new Engine();

try {
    $workflow = Workflow::fromFile(__DIR__ . '/workflows/basic_workflow.ai', [
        'inputs' => [
            'customer_name' => 'Rafael',
            'order_total' => 149.90,
        ],
    ]);

    $response = $engine->run($workflow);

    if ($response->isError()) {
        print "Execution failed:\n";
        print_r($response->error());

        return;
    }

    print "Execution succeeded:\n";
    print_r($response->success());
} finally {
    $engine->close();
}
