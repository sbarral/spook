// Refresh on WTFiles events.
var evtSource = new EventSource("http://localhost:2133/events"); 
evtSource.addEventListener("update", function(e) {
    // Save current page hash into local storage
    localStorage.setItem('pageHash', window.location.hash);
    // Save last scroll position into local storage.
    localStorage.setItem('scrollPosition', window.scrollY);
    window.location.reload(true);
});

// Jump to the last saved scroll position.
window.addEventListener('load', function() {
    // Jump to saved page hash
    location.replace(window.location.hash = localStorage.getItem('pageHash'));
    if(localStorage.getItem('scrollPosition') !== null)
        window.scrollTo(0, localStorage.getItem('scrollPosition'));
});
