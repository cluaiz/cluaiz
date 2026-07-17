import { mountDashboard } from '/src/app/dashboard/dashboard.js?v=3';
import { mountApiWorkspace } from '/src/app/api/script.js';
import { mountHubWorkspace } from '/src/app/hub/script.js';

window.getApiBaseUrl = function() {
    return window.location.origin;
};

async function route() {
    const path = window.location.pathname;
    const root = document.getElementById('app-root');
    
    // Check if we are transitioning away from settings
    if (path !== '/setting' && path !== '/settings') {
        const settingsMount = document.getElementById('settings-app-mount');
        if (settingsMount) settingsMount.remove();
        root.style.display = 'flex'; // Ensure root is visible
    }

    root.innerHTML = ''; // Clear existing content

    if (path === '/' || path === '/chat') {
        await mountDashboard(root);
    } else if (path === '/api') {
        await mountApiWorkspace(root);
    } else if (path === '/hub') {
        await mountHubWorkspace(root);
    } else if (path === '/setting' || path === '/settings') {
        await mountDashboard(root);
        import('/src/app/setting/setting.js?v=' + new Date().getTime()).then(module => {
            module.mountSettings();
        }).catch(err => console.error("Failed to load settings module:", err));
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
// Call route directly since this is a module script (deferred) 
// and the DOM is already parsed when this runs.
route();
