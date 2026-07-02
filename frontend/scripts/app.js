// =====================================================================================================================
// GET THE HTML ELEMENTS
// =====================================================================================================================
const nav_bar = document.querySelector('#nav_bar');
const graph = document.querySelector('#graph_content');
const clock_time = document.querySelector('#clock_time');
const session_list = document.querySelector('#session_list');
const close_button = document.querySelector('#close_button');
const toggle_button = document.querySelector('#toggle_button');
const tracker_name_input = document.querySelector('#tracker_name_input');
const trackers_selection_menu = document.querySelector('#trackers_selection_menu');
const creation_buttons = {
    'CREATE': document.querySelector('#create_tracker_button'),
    'CANCEL': document.querySelector('#cancel_creation_button'),
}
const PAGES = {
    'APP': document.querySelector('#app'),
    'LOADING': document.querySelector('#loading_page'),
}
const APP_PAGES = {
    'SELECTION': document.querySelector('#tracker_selection_screen'),
    'CREATION': document.querySelector('#tracker_creation_screen'),
    'TRACKER': document.querySelector('#tracker_screen'),
}

// FREEZE THE OBJECTS
Object.freeze(creation_buttons);
Object.freeze(PAGES);
Object.freeze(APP_PAGES);

// =====================================================================================================================
// ELEMENT CREATION
// =====================================================================================================================
// Back Button
const back_button = document.createElement('button');
back_button.innerHTML = 'Back';
back_button.classList.add('action_btn_txt');
back_button.classList.add('bordered');
add_border(back_button);

// Create Tracker Button
const create_tracker_button = document.createElement('button')
create_tracker_button.innerHTML = 'Create Tracker';
create_tracker_button.classList.add('action_btn_txt');
create_tracker_button.classList.add('bordered');
add_border(create_tracker_button);
nav_bar.prepend(create_tracker_button);

// =====================================================================================================================
// GLOBAL VARIABLES
// =====================================================================================================================
let trackers;
let selected_tracker;
let current_page = PAGES.LOADING;

// =====================================================================================================================
// Function Definitions
// =====================================================================================================================
// function to format a timestamp into a human readable format
let format_timestamp = (timestamp) => {
    let output = "";

    let seconds = timestamp % 60;
    timestamp = Math.floor(timestamp / 60);
    let minutes = timestamp % 60;
    timestamp = Math.floor(timestamp / 60);
    let hours = timestamp % 24;
    timestamp = Math.floor(timestamp / 24);

    let days = timestamp % 365;
    timestamp = Math.floor(timestamp / 365);

    output += timestamp + "y ";
    output += days + "d ";
    output += hours + "h ";
    output += minutes + "m ";
    output += seconds + "s";

    return output;
}

// function to populate the session list with the sessions of the selected tracker
let populate_session_list = () => {
    // Reset the session list
    session_list.innerHTML = '';

    // populate the session list with the sessions of the selected tracker
    for (let session of selected_tracker.sessions) {
        let session_el = document.createElement('div');
        session_el.classList.add('session');
        session_el.innerHTML = 'Started on' + new Date(session.start_time * 1000).toLocaleString() + ', Lasted for ' + format_timestamp(session.duration);
        session_list.appendChild(session_el);
    }
}

// function to populate the graph with the time per day of the selected tracker
let populate_graph = () => {
    // Reset the graph
    graph.innerHTML = '';

    // Get the max duration of the selected tracker
    let max_duration = 1;
    for (let day of selected_tracker.time_per_day) {
        if (day.time > max_duration) {
            max_duration = day.time;
        }
    }

    // loop through the time per day of the selected tracker and populate the graph
    for (let day of selected_tracker.time_per_day) {
        const bar = document.createElement('div');
        bar.classList.add('bar');

        let day_index = document.createElement('p');
        day_index.innerHTML = day.day_index;
        bar.appendChild(day_index);

        let bar_marker_container = document.createElement('div');
        bar_marker_container.classList.add('bar_marker-container');

        let bar_marker = document.createElement('div');
        bar_marker.classList.add('bar_marker');
        bar_marker.style.height = 100 / max_duration * day.time + '%';
        bar_marker_container.appendChild(bar_marker);

        bar_marker_container.appendChild(bar_marker);

        bar.appendChild(day_index);
        bar.appendChild(bar_marker_container);

        graph.appendChild(bar);
    }
}

// Function to kill the server
let kill_server = async () => {
    // Send a POST request to the /kill endpoint
    let response = await fetch('/kill', {
        method: 'POST',
    });
    if (!response.ok) {
        alert('Error killing server');
    } else {
        close();
    }
}

// Function to toggle between pages
let toggle_page = (page, change_current_page = true) => {
    if (current_page === page) {
        return;
    }

    for (let possible_page of Object.values(PAGES)) {
        possible_page.style.display = 'none';
    }

    if (page !== PAGES.LOADING) {
        nav_bar.style.display = 'flex';
    } else {
        nav_bar.style.display = 'none';
    }

    page.style.display = 'flex';

    if (change_current_page) {
        current_page = page;
    }
}

// Function to get the current tracker time
let get_current_tracker_time = () => {
    if (selected_tracker === null || selected_tracker === undefined) {
        return '00:00:00';
    }


    let events = selected_tracker.events;
    let sessions = selected_tracker.sessions;
    let total_time = 0;

    for (let session of sessions) {
        total_time += session.duration;
    }

    if (events.length > 0) {
        if (events[events.length - 1].event_type.toLowerCase() === 'start') {
            total_time += Math.floor(Date.now() / 1000) - events[events.length - 1].timestamp;
        }
    }

    return format_timestamp(total_time);
}

// Function to sleep for a specified number of milliseconds
const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));

// Function to run the clock
let start_clock = async () => {
    if (current_page !== APP_PAGES.TRACKER) {
        return;
    }

    clock_time.innerHTML = get_current_tracker_time();

    await sleep(100);
    await start_clock();
}

// Function to toggle between pages
let toggle_app_page = (page) => {
    if (current_page === page) {
        return;
    }

    for (let possible_page of Object.values(APP_PAGES)) {
        possible_page.style.display = 'none';
    }

    page.style.display = 'flex';

    if (page === APP_PAGES.SELECTION) {
        nav_bar.prepend(create_tracker_button);
    } else if (nav_bar.contains(create_tracker_button)) {
        nav_bar.removeChild(create_tracker_button);
    }

    if (page === APP_PAGES.TRACKER) {
        nav_bar.prepend(back_button);
        document.querySelector('#tracker_name').innerHTML = selected_tracker.name;
        populate_graph();
        populate_session_list();
    } else if (nav_bar.contains(back_button)) {
        nav_bar.removeChild(back_button);
        graph.innerHTML = '';
    }

    current_page = page;

    toggle_page(PAGES.APP, false);
}

// Function to Fetch the trackers
let get_trackers = async () => {
    let response = await fetch('/trackers');
    if (!response.ok) {
        alert('Error getting trackers');
    } else {
        trackers = await response.json();
        toggle_app_page(APP_PAGES.SELECTION);
    }
}

// Function to update the tracker selection menu
let update_trackers = async () => {

    await get_trackers();

    trackers_selection_menu.innerHTML = '';

    for (let tracker of trackers) {

        let current_name;

        if (selected_tracker) {
            current_name = selected_tracker.name;
        }


        toggle_page(PAGES.LOADING);
        let div = document.createElement('div');
        let name = document.createElement('p');

        let formatted = new Date(tracker.created_at * 1000);

        name.innerHTML = tracker.name + ', Created on: ' + formatted.toLocaleString();
        name.onclick = () => {
            toggle_app_page(APP_PAGES.TRACKER);
            start_clock();
        }

        name.addEventListener('mouseover', () => {
            selected_tracker = tracker;
        })

        const delete_button = document.createElement('button');
        delete_button.innerHTML = 'Delete';
        delete_button.classList.add('action_btn_txt');
        delete_button.classList.add('bordered');
        delete_button.addEventListener('click', async () => {
            let response = await fetch('/tracker', {
                    method: 'DELETE',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({
                        name: tracker.name,
                    }),
                }
            );
            if (response.ok) {
                await update_trackers();
            } else {
                alert('Error deleting tracker:' + response.statusText);
            }
            toggle_app_page(APP_PAGES.SELECTION);
        })

        add_border(name);
        add_border(delete_button);

        div.appendChild(name);
        div.appendChild(delete_button);

        name.classList.add('bordered', 'action_btn_txt');

        div.addEventListener('click', () => {
            selected_tracker = tracker;
        })

        div.classList.add('tracker_option');

        if (tracker.name === current_name) {
            selected_tracker = tracker;
        }

        trackers_selection_menu.appendChild(div);
    }

    add_border(trackers_selection_menu);
}

// function to handle the initial load (getting the trackers and showing the selection screen
let initial_load = async () => {
    await update_trackers();
    toggle_app_page(APP_PAGES.SELECTION);
}

// function to handle creating a new tracker
let create_tracker = async () => {
    toggle_page(PAGES.LOADING);
    let already_exists = trackers.find(tracker => tracker.name === tracker_name_input.value);
    if (already_exists) {
        window.alert('Tracker already exists');
        return;
    }

    let response = await fetch('/tracker', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            name: tracker_name_input.value,
        }),
    });

    if (!response.ok) {
        alert('Error creating tracker:' + await response.json());
    } else {
        await update_trackers();
        toggle_app_page(APP_PAGES.SELECTION);
    }
}

// function to handle cancelling the creation of a new tracker
let cancel_creation = () => {
    tracker_name_input.value = '';
    toggle_app_page(APP_PAGES.SELECTION);
}

// function to handle toggling the tracker
let toggle_tracker = async () => {
    let response = await fetch('/toggle', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            name: selected_tracker.name,
        }),
    });

    if (response.ok) {
        await update_trackers();
        toggle_app_page(APP_PAGES.TRACKER);
    } else {
        alert('Error toggling tracker');
        console.log((await response).text());
    }

}

// =====================================================================================================================
// EVENT LISTENERS
// =====================================================================================================================
window.addEventListener('load', initial_load);
creation_buttons.CANCEL.addEventListener('click', toggle_app_page.bind(null, APP_PAGES.SELECTION));
creation_buttons.CREATE.addEventListener('click', create_tracker);
creation_buttons.CANCEL.addEventListener('click', cancel_creation);
close_button.addEventListener('click', kill_server);
create_tracker_button.addEventListener('click', toggle_app_page.bind(null, APP_PAGES.CREATION));
back_button.addEventListener('click', toggle_app_page.bind(null, APP_PAGES.SELECTION));
toggle_button.addEventListener('click', toggle_tracker);