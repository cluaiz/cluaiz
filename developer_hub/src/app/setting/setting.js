let currentModule = null;

export async function mountSettings(rootElement) {
    try {
        const response = await fetch('/src/app/setting/setting.html?v=' + new Date().getTime());
        const html = await response.text();

        // Ensure CSS is loaded
        if (!document.getElementById('setting-css')) {
            const link = document.createElement('link');
            link.id = 'setting-css';
            link.rel = 'stylesheet';
            link.href = '/src/app/setting/setting.css?v=' + new Date().getTime();
            document.head.appendChild(link);
        }

        // We'll append settings over the dashboard, acting as a full-screen SPA route
        // but we'll just prepend it to document.body and hide the main root to avoid destroying state
        const settingsWrapper = document.createElement('div');
        settingsWrapper.id = 'settings-app-mount';
        settingsWrapper.innerHTML = html;
        document.body.appendChild(settingsWrapper);

        // Hide dashboard
        const appRoot = document.getElementById('app-root');
        if (appRoot) {
            appRoot.style.display = 'none';
        }

        if (window.lucide) window.lucide.createIcons();

        setupSettingsRouter();

        // Load default tab from URL or fallback to general_setting
        const searchParams = new URLSearchParams(window.location.search);
        let defaultTab = searchParams.get('tab');
        if (!defaultTab) {
            defaultTab = 'general_setting';
            const newUrl = new URL(window.location);
            newUrl.searchParams.set('tab', defaultTab);
            window.history.replaceState({}, '', newUrl);
        }

        // Set the active button
        const tabBtns = document.querySelectorAll('.nav-tab[data-module]');
        tabBtns.forEach(t => t.classList.remove('active'));
        const activeBtn = document.querySelector(`.nav-tab[data-module="${defaultTab}"]`);
        if (activeBtn) activeBtn.classList.add('active');

        // Load the module
        loadModule(defaultTab);

    } catch (e) {
        console.error("Failed to load settings:", e);
    }
}

export function unmountSettings() {
    const settingsWrapper = document.getElementById('settings-app-mount');
    if (settingsWrapper) {
        settingsWrapper.remove();
    }
    const appRoot = document.getElementById('app-root');
    if (appRoot) {
        appRoot.style.display = 'flex'; // restore dashboard
    }

    // Change URL back to root if we were at /setting
    if (window.location.pathname === '/setting' || window.location.pathname === '/settings') {
        window.history.pushState({}, '', '/');
    }
}

function setupSettingsRouter() {
    const closeBtn = document.querySelector('.close-settings-btn');
    if (closeBtn) {
        closeBtn.addEventListener('click', unmountSettings);
    }

    const tabs = document.querySelectorAll('.nav-tab[data-module]');
    tabs.forEach(tab => {
        tab.addEventListener('click', (e) => {
            // Remove active from all
            tabs.forEach(t => t.classList.remove('active'));
            // Add active to clicked
            const btn = e.currentTarget;
            btn.classList.add('active');

            const moduleName = btn.getAttribute('data-module');
            
            // Update URL without reloading
            const newUrl = new URL(window.location);
            newUrl.searchParams.set('tab', moduleName);
            window.history.pushState({}, '', newUrl);

            loadModule(moduleName);
        });
    });
}

async function loadModule(moduleName) {
    if (currentModule === moduleName) return;
    currentModule = moduleName;

    const contentArea = document.getElementById('settings-content-area');
    if (!contentArea) return;

    // Show loading state
    contentArea.innerHTML = `<div class="flex-center w-full h-full text-muted">Loading module...</div>`;

    try {
        // Load HTML
        const htmlRes = await fetch(`/src/app/setting/${moduleName}/${moduleName}.html?v=` + new Date().getTime());
        if (!htmlRes.ok) throw new Error(`Module HTML not found: ${moduleName}`);
        const htmlContent = await htmlRes.text();
        contentArea.innerHTML = htmlContent;

        // Load CSS dynamically if it exists
        const cssId = `css-${moduleName}`;
        if (!document.getElementById(cssId)) {
            const link = document.createElement('link');
            link.id = cssId;
            link.rel = 'stylesheet';
            link.href = `/src/app/setting/${moduleName}/${moduleName}.css?v=` + new Date().getTime();
            // We append to head, but we could also just let it persist
            document.head.appendChild(link);
        }

        // Load JS dynamically if it exists
        try {
            const moduleJs = await import(`/src/app/setting/${moduleName}/${moduleName}.js?v=` + new Date().getTime());
            if (moduleJs && moduleJs.mount) {
                moduleJs.mount(contentArea);
            }
        } catch (jsErr) {
            console.warn(`No JS found or failed to load for ${moduleName}:`, jsErr);
        }

        if (window.lucide) window.lucide.createIcons();

    } catch (e) {
        contentArea.innerHTML = `<div class="settings-module"><h2 class="settings-section-title" style="color: #ef4444;">Error Loading Module</h2><p class="text-muted">${e.message}</p></div>`;
    }
}
