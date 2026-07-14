export async function mountDashboard(rootElement) {
    try {
        const response = await fetch('/src/app/dashboard/dashboard.html?v=' + new Date().getTime());
        const html = await response.text();
        rootElement.innerHTML = html;
        
        // Add CSS if not already present
        if (!document.getElementById('dashboard-css')) {
            const link = document.createElement('link');
            link.id = 'dashboard-css';
            link.rel = 'stylesheet';
            link.href = '/src/app/dashboard/dashboard.css?v=' + new Date().getTime();
            document.head.appendChild(link);
        }

        setupDashboardLogic();
    } catch (e) {
        rootElement.innerHTML = `<h2 style="color:red; padding: 20px;">Failed to load dashboard: ${e.message}</h2>`;
    }
}

function setupDashboardLogic() {
    const settingsBtn = document.querySelector('.settings-btn');
    if (settingsBtn) {
        settingsBtn.addEventListener('click', () => {
            import('/src/app/setting/setting.js?v=' + new Date().getTime()).then(module => {
                module.mountSettings();
            }).catch(err => {
                console.error("Failed to load settings module:", err);
            });
        });
    }

    const incognitoBtn = document.querySelector('.incognito-btn');
    if (incognitoBtn) {
        incognitoBtn.addEventListener('click', () => console.log("Temporary Chat Session Started. No telemetry will be saved."));
    }

    // Card click → route navigation
    const apiToolkitCard = document.getElementById('api-toolkit');
    if (apiToolkitCard) {
        apiToolkitCard.addEventListener('click', () => window.navigateTo('/api'));
    }

    const hubToolkitCard = document.getElementById('hub-toolkit');
    if (hubToolkitCard) {
        hubToolkitCard.addEventListener('click', () => window.navigateTo('/hub'));
    }

    // Mount Chat Stream (messages area)
    const chatStreamMount = document.getElementById('chat-stream-mount-point');
    if (chatStreamMount) {
        import('/src/app/chat/chat_stream/chat_stream.js?v=' + new Date().getTime()).then(module => {
            module.mountChatStream(chatStreamMount);
        }).catch(err => {
            console.error("Failed to load chat stream module:", err);
        });
    }

    // Mount Chat Input
    const chatMount = document.getElementById('chat-mount-point');
    if (chatMount) {
        import('/src/app/chat/chat_input/chat.js?v=' + new Date().getTime()).then(module => {
            module.mountChat(chatMount);
        }).catch(err => {
            console.error("Failed to load chat module:", err);
        });
    }
}
