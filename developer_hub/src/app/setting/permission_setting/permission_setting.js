export function mount(container) {
    const toggles = container.querySelectorAll('.setting-toggle');
    toggles.forEach(toggle => {
        toggle.addEventListener('click', (e) => {
            e.currentTarget.classList.toggle('active');
            // Save state
        });
    });

    const selects = container.querySelectorAll('select');
    selects.forEach(select => {
        select.addEventListener('change', (e) => {
            console.log(`Setting changed: ${e.target.id} = ${e.target.value}`);
            // Save state
        });
    });
}
