<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

enum WorkflowToolResultStatus: string
{
    case Success = 'success';

    case Error = 'error';
}
