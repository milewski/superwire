<?php

header('Content-Type: application/json');

$city = isset($_GET['city']) && $_GET['city'] !== '' ? $_GET['city'] : 'Madrid';
$weatherUrl = 'https://wttr.in/' . rawurlencode($city) . '?format=%C+%t';
$weatherSummary = @file_get_contents($weatherUrl);

if ($weatherSummary === false) {
    $weatherSummary = 'Weather service temporarily unavailable';
}

echo json_encode([
    'city' => $city,
    'summary' => trim($weatherSummary),
    'source' => 'wttr.in via php-shim',
], JSON_UNESCAPED_UNICODE);
