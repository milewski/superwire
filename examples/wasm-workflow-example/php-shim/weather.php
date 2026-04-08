<?php

header('Content-Type: application/json');

if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    http_response_code(405);

    echo json_encode(['error' => 'method not allowed'], JSON_UNESCAPED_UNICODE);

    exit;
}

$requestBody = file_get_contents('php://input');
$decodedRequestBody = json_decode($requestBody, true);

if (!is_array($decodedRequestBody)) {
    http_response_code(400);

    echo json_encode(['error' => 'invalid request payload'], JSON_UNESCAPED_UNICODE);

    exit;
}

$agentInput = $decodedRequestBody['agent_input'] ?? [];
$boundInput = $decodedRequestBody['bound_input'] ?? [];

$city = $boundInput['city'] ?? ($agentInput['city'] ?? 'Madrid');
$weatherUrl = 'https://wttr.in/' . rawurlencode((string) $city) . '?format=%C+%t';
$weatherSummary = @file_get_contents($weatherUrl);

if ($weatherSummary === false) {
    $weatherSummary = 'Weather service temporarily unavailable';
}

echo json_encode([
    'city' => (string) $city,
    'summary' => trim($weatherSummary),
    'source' => 'wttr.in via php-shim',
], JSON_UNESCAPED_UNICODE);
