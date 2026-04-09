<?php

namespace Superwire\Laravel\Security;

use Illuminate\Contracts\Config\Repository;
use Illuminate\Http\Request;
use Symfony\Component\HttpKernel\Exception\AccessDeniedHttpException;

final readonly class InternalRequestGuard
{
    public function __construct(
        private Repository $config,
    )
    {
    }

    public function assertAuthorized(Request $request): void
    {
        if ($this->config->get('superwire.security.enforce_localhost_only', true)) {
            $remoteAddress = (string) $request->server->get('REMOTE_ADDR', '');

            if (!in_array($remoteAddress, [ '127.0.0.1', '::1' ], true)) {
                throw new AccessDeniedHttpException('forbidden remote address');
            }
        }

        $expectedToken = (string) $this->config->get('superwire.runtime.internal_token', '');

        if (blank($expectedToken)) {
            return;
        }

        $providedToken = (string) $request->headers->get('x-superwire-internal-token', '');

        if (blank($providedToken) || !hash_equals($expectedToken, $providedToken)) {
            throw new AccessDeniedHttpException('invalid internal token');
        }
    }
}
