import { mountDashboard } from '/src/app/dashboard/dashboard.js?v=2';
import { mountApiWorkspace } from '/src/app/api/script.js';

function route() {
    const path = window.location.pathname;
    const root = document.getElementById('app-root');
    root.innerHTML = ''; // Clear existing content

    if (path === '/') {
        mountDashboard(root);
    } else if (path === '/api') {
        mountApiWorkspace(root);
    } else {
        root.innerHTML = `<h1 style="color: white; padding: 40px;">404 Not Found</h1>`;
    }
}

// Global pushState override to handle client-side navigation without page reload
window.navigateTo = (path) => {
    window.history.pushState({}, '', path);
    route();
};

window.addEventListener('popstate', route);
document.addEventListener('DOMContentLoaded', route);
