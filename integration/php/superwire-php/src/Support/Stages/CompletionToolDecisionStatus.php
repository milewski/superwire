<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Stages;

enum CompletionToolDecisionStatus: string
{
    case Continue = 'continue';
    case Success = 'success';
    case Error = 'error';
}
