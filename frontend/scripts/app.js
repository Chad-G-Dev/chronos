let close_button = document.getElementById('close-button');

let kill_server = async () => {
    let response = await fetch('/kill', {
        method: 'POST',
    });
    if (!response.ok) {
        alert('Error killing server');
    } else {
        close();
    }
}

close_button.addEventListener('click', kill_server);