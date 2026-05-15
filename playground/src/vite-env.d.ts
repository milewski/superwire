/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue';

  const component: DefineComponent<object, object, unknown>;
  export default component;
}

declare module '*.wire?raw' {
  const source: string;
  export default source;
}

declare module '*.json?raw' {
  const source: string;
  export default source;
}
