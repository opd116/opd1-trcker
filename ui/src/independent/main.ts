import { mount } from 'svelte';
import './style.css';
if (import.meta.env.DEV) {
  import('./App.svelte').then(({ default: App }) => mount(App, { target: document.getElementById('app')! }));
}
