import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './style.css';

const appElement = document.getElementById('app');

if (!appElement) {
  throw new Error('Missing #app mount element.');
}

createRoot(appElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
