<?php

declare(strict_types=1);

use App\Superwire\Http\ToolController;

require_once __DIR__ . '/../app/Superwire/Tool.php';
require_once __DIR__ . '/../app/Superwire/Tools/WeatherTool.php';
require_once __DIR__ . '/../app/Superwire/Security/InternalRequestGuard.php';
require_once __DIR__ . '/../app/Superwire/Http/ToolController.php';

header('Content-Type: application/json');

if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    http_response_code(405);

    echo json_encode(['error' => 'method_not_allowed'], JSON_UNESCAPED_UNICODE);

    exit;
}

$requestUri = (string) ($_SERVER['REQUEST_URI'] ?? '/');
$requestPath = parse_url($requestUri, PHP_URL_PATH);

if (!is_string($requestPath)) {
    http_response_code(400);

    echo json_encode(['error' => 'invalid_path'], JSON_UNESCAPED_UNICODE);

    exit;
}

if (!preg_match('#^/superwire/tools/([a-zA-Z0-9_\-]+)/execute$#', $requestPath, $pathMatch)) {
    http_response_code(404);

    echo json_encode(['error' => 'route_not_found'], JSON_UNESCAPED_UNICODE);

    exit;
}

$toolName = $pathMatch[1];
$requestBody = file_get_contents('php://input');

if ($requestBody === false) {
    http_response_code(400);

    echo json_encode(['error' => 'invalid_request_body'], JSON_UNESCAPED_UNICODE);

    exit;
}

$controller = new ToolController();

try {
    $responsePayload = $controller->execute($toolName, $_SERVER, $requestBody);

    echo json_encode($responsePayload, JSON_UNESCAPED_UNICODE);
} catch (RuntimeException $runtimeException) {
    http_response_code(403);

    echo json_encode(['error' => $runtimeException->getMessage()], JSON_UNESCAPED_UNICODE);
}
