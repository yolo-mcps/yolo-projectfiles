// This is a completely new version of the file
function newLogger() {
    return {
        log: (msg) => console.log(`[LOG] ${msg}`),
        error: (msg) => console.error(`[ERROR] ${msg}`),
        debug: (msg) => console.debug(`[DEBUG] ${msg}`)
    };
}