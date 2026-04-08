<?php

namespace App\Superwire\Security;

use RuntimeException;

final class InternalRequestGuard
{
    public static function assertAuthorized(array $server, ?string $bodyToken = null): void
    {
        $remoteAddress = (string) ($server['REMOTE_ADDR'] ?? '');

        if (!in_array($remoteAddress, ['127.0.0.1', '::1'], true)) {
            throw new RuntimeException('forbidden_remote_address');
        }

        $expectedToken = getenv('SUPERWIRE_INTERNAL_TOKEN') ?: '';

        if ($expectedToken === '') {
            return;
        }

        $providedHeaderToken = (string) ($server['HTTP_X_SUPERWIRE_INTERNAL_TOKEN'] ?? '');
        $providedBodyToken = $bodyToken ?? '';

        if ($providedHeaderToken !== '' && hash_equals($expectedToken, $providedHeaderToken)) {
            return;
        }

        if ($providedBodyToken === '' || !hash_equals($expectedToken, $providedBodyToken)) {
            throw new RuntimeException('forbidden_internal_token');
        }
    }
}
