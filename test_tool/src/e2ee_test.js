import axios from 'axios';
import WebSocket from 'ws';
import { CONFIG, generateUsername } from './config.js';

const api = axios.create({ baseURL: CONFIG.API_URL });

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function getToken(username, password) {
    const res = await api.post('/api/auth/login', { username, password });
    return res.data.token;
}

async function registerUser(username, password) {
    await api.post('/api/auth/register', { username, password });
}

function createFakeKey() {
    // Generate 32 random bytes and URL-safe base64 encode (no padding)
    const bytes = new Uint8Array(32);
    crypto.getRandomValues(bytes);
    return Buffer.from(bytes).toString('base64url');
}

function createFakeSignature() {
    // Generate 64 random bytes for signature
    const bytes = new Uint8Array(64);
    crypto.getRandomValues(bytes);
    return Buffer.from(bytes).toString('base64url');
}

async function uploadKeys(token) {
    await api.post('/keys/upload', {
        identity_key: createFakeKey(),
        registration_id: Math.floor(Math.random() * 1000),
        signed_pre_key: {
            key_id: 1,
            public_key: createFakeKey(),
            signature: createFakeSignature(),
        },
        one_time_keys: [
            { key_id: 1, public_key: createFakeKey() },
            { key_id: 2, public_key: createFakeKey() },
            { key_id: 3, public_key: createFakeKey() },
        ],
    }, {
        headers: { Authorization: `Bearer ${token}` }
    });
}

async function testE2EEKeyExchange() {
    console.log('\n[TEST] E2EE Key Exchange Flow');
    
    const aliceName = 'alice_' + Math.random().toString(36).substring(2, 8);
    const bobName = 'bob_' + Math.random().toString(36).substring(2, 8);
    const password = 'testpass123';
    
    console.log(`  Creating users: ${aliceName}, ${bobName}`);
    
    // Register both users
    await registerUser(aliceName, password);
    await registerUser(bobName, password);
    
    // Login to get tokens
    const aliceToken = await getToken(aliceName, password);
    const bobToken = await getToken(bobName, password);
    
    // Both upload their keys
    console.log('  Uploading keys...');
    await uploadKeys(aliceToken);
    await uploadKeys(bobToken);
    
    // Alice claims Bob's keys
    console.log(`  Alice claims ${bobName}'s keys...`);
    const claimRes = await api.get(`/keys/claim/${bobName}`, {
        headers: { Authorization: `Bearer ${aliceToken}` }
    });
    
    const devices = claimRes.data.devices;
    console.log(`  Found ${devices.length} device(s) for ${bobName}`);
    
    if (devices.length > 0) {
        const device = devices[0];
        console.log(`  Device ID: ${device.device_id}`);
        console.log(`  OTK remaining: ${device.one_time_keys_remaining}`);
        console.log(`  Has OTK: ${device.one_time_key !== null}`);
    }
    
    // Bob lists his devices
    console.log(`  Bob lists his devices...`);
    const listRes = await api.get('/keys/devices', {
        headers: { Authorization: `Bearer ${bobToken}` }
    });
    console.log(`  Bob has ${listRes.data.length} device(s)`);
    
    return true;
}

async function testKeyValidation() {
    console.log('\n[TEST] Key Validation (invalid keys should be rejected)');
    
    const username = 'keytest_' + Math.random().toString(36).substring(2, 8);
    await registerUser(username, 'testpass123');
    const token = await getToken(username, 'testpass123');
    
    // Try with invalid key (too short)
    try {
        await api.post('/keys/upload', {
            identity_key: 'tooshort',
            registration_id: 42,
            signed_pre_key: {
                key_id: 1,
                public_key: 'tooshort',
                signature: createFakeSignature(),
            },
            one_time_keys: [],
        }, {
            headers: { Authorization: `Bearer ${token}` }
        });
        console.log('  [FAIL] Invalid key was accepted!');
        return false;
    } catch (err) {
        console.log(`  [OK] Invalid key rejected: ${err.response?.status}`);
        return true;
    }
}

async function runE2EETests() {
    console.log('='.repeat(50));
    console.log('E2EE TESTS');
    console.log('='.repeat(50));
    
    let ok = true;
    
    try {
        ok = await testE2EEKeyExchange() && ok;
        ok = await testKeyValidation() && ok;
        
        console.log('\n[OK] E2EE tests completed!');
        return ok;
    } catch (err) {
        console.error(`\n[FAIL] E2EE test failed: ${err.message}`);
        if (err.response) {
            console.error(`  Status: ${err.response.status}`);
            console.error(`  Data: ${JSON.stringify(err.response.data)}`);
        }
        return false;
    }
}

export { runE2EETests };

if (import.meta.url === `file://${process.argv[1]}`) {
    runE2EETests().then(ok => process.exit(ok ? 0 : 1));
}
