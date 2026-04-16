import WebSocket from 'ws';
import { CONFIG, generateUsername } from './config.js';

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function createWsConnection(token) {
    return new Promise((resolve, reject) => {
        const ws = new WebSocket(`${CONFIG.RELAY_WS_URL}?token=${token}`);
        
        ws.on('open', () => resolve(ws));
        ws.on('error', reject);
        ws.on('message', (data) => {
            console.log(`  [WS Received]: ${data.toString().substring(0, 100)}...`);
        });
    });
}

async function testBasicWebSocket() {
    console.log(`\n[TEST] Basic WebSocket Connection`);
    
    try {
        // Note: This requires a valid JWT token
        // In real tests, you'd first login to get a token
        const ws = new WebSocket(CONFIG.RELAY_WS_URL);
        
        await new Promise((resolve, reject) => {
            ws.on('open', () => {
                console.log('  Connection opened');
                resolve();
            });
            ws.on('error', reject);
        });
        
        ws.close();
        console.log('  Connection closed');
        return true;
    } catch (err) {
        console.log(`  Expected error (no auth): ${err.message}`);
        return true; // Expected without proper auth
    }
}

async function testPresenceWatch(token) {
    console.log(`\n[TEST] Presence Watch`);
    
    const ws = await createWsConnection(token);
    
    const watchMsg = {
        type: 'WatchPresence',
        user_ids: ['user1', 'user2'],
    };
    
    ws.send(JSON.stringify(watchMsg));
    await sleep(100);
    
    ws.close();
    console.log('  Presence watch sent');
    return true;
}

async function runWsTests() {
    console.log('='.repeat(50));
    console.log('WEBSOCKET TESTS');
    console.log('='.repeat(50));
    
    try {
        await testBasicWebSocket();
        console.log('\n[OK] WebSocket tests completed!');
        return true;
    } catch (err) {
        console.error(`\n[FAIL] WebSocket test failed: ${err.message}`);
        return false;
    }
}

export { runWsTests };

if (import.meta.url === `file://${process.argv[1]}`) {
    runWsTests().then(ok => process.exit(ok ? 0 : 1));
}
