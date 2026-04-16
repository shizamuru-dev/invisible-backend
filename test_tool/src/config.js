export const CONFIG = {
    API_URL: process.env.API_URL || 'http://localhost:8080',
    RELAY_WS_URL: process.env.RELAY_WS_URL || 'ws://localhost:8081',
    TEST_USERNAME_PREFIX: 'testuser_',
};

export function generateUsername() {
    return CONFIG.TEST_USERNAME_PREFIX + Math.random().toString(36).substring(2, 10);
}
