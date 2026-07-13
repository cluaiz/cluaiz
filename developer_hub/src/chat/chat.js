export function handleChatKeyPress(event) {
    if (event.key === 'Enter') {
        sendChatMessage();
    }
}

export async function sendChatMessage() {
    const input = document.getElementById('main-chat-input');
    const text = input.value.trim();
    if (!text) return;
    
    // Hide shortcuts, show chat history
    const shortcuts = document.getElementById('dashboard-shortcuts');
    if (shortcuts && !shortcuts.classList.contains('hidden')) {
        shortcuts.style.opacity = '0';
        setTimeout(() => {
            shortcuts.classList.add('hidden');
            document.getElementById('view-chat-history').classList.remove('hidden');
        }, 300);
    }

    const chatHistory = document.getElementById('view-chat-history');
    
    // Add user message
    const userMsg = document.createElement('div');
    userMsg.className = 'chat-message user';
    userMsg.textContent = text;
    chatHistory.appendChild(userMsg);
    
    input.value = '';
    
    // AI Placeholder
    const aiMsg = document.createElement('div');
    aiMsg.className = 'chat-message assistant markdown-body';
    aiMsg.textContent = 'Thinking...';
    chatHistory.appendChild(aiMsg);
    chatHistory.scrollTop = chatHistory.scrollHeight;

    try {
        const response = await fetch('http://localhost:8000/v1/chat/completions', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                model: "default",
                messages: [{ role: "user", content: text }],
                stream: false
            })
        });
        const data = await response.json();
        if (data.choices && data.choices[0] && data.choices[0].message) {
            aiMsg.innerHTML = marked.parse(data.choices[0].message.content);
        } else {
            aiMsg.textContent = "Error: Invalid response format";
        }
    } catch (e) {
        aiMsg.textContent = "Network error: " + e.message;
    }
    chatHistory.scrollTop = chatHistory.scrollHeight;
}
