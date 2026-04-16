import axios from 'axios';
import { CONFIG, generateUsername } from './config.js';

const api = axios.create({ baseURL: CONFIG.API_URL });

async function testRegisterLogin() {
    const username = generateUsername();
    const password = 'testpassword123';
    
    console.log(`\n[TEST] Register/Login: ${username}`);
    
    // Register
    const regRes = await api.post('/api/auth/register', { username, password });
    console.log(`  Register: ${regRes.status} - ${regRes.data}`);
    
    // Login
    const loginRes = await api.post('/api/auth/login', { username, password });
    console.log(`  Login: ${loginRes.status}`);
    const token = loginRes.data.token;
    
    return { username, token };
}

async function testKeysUpload(token, username) {
    console.log(`\n[TEST] Upload Keys for ${username}`);
    
    // Generate valid 32-byte URL-safe base64 keys (without padding)
    const generateKey = () => Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString('base64url');
    const generateSignature = () => Buffer.from(crypto.getRandomValues(new Uint8Array(64))).toString('base64url');
    
    const identityKey = generateKey();
    const publicKey = generateKey();
    const signature = generateSignature();
    
    const res = await api.post('/keys/upload', {
        identity_key: identityKey,
        registration_id: 42,
        signed_pre_key: {
            key_id: 1,
            public_key: publicKey,
            signature: signature,
        },
        one_time_keys: [
            { key_id: 1, public_key: publicKey },
            { key_id: 2, public_key: publicKey },
        ],
    }, {
        headers: { Authorization: `Bearer ${token}` }
    });
    
    console.log(`  Upload Keys: ${res.status}`);
    return true;
}

async function testListDevices(token) {
    console.log(`\n[TEST] List Devices`);
    
    const res = await api.get('/keys/devices', {
        headers: { Authorization: `Bearer ${token}` }
    });
    
    console.log(`  List Devices: ${res.status} - ${res.data.length} device(s)`);
    return res.data;
}

async function testClaimKeys(token, targetUsername) {
    console.log(`\n[TEST] Claim Keys for ${targetUsername}`);
    
    const res = await api.get(`/keys/claim/${targetUsername}`, {
        headers: { Authorization: `Bearer ${token}` }
    });
    
    console.log(`  Claim Keys: ${res.status} - ${res.data.devices?.length || 0} device(s)`);
    return res.data;
}

async function runApiTests() {
    console.log('='.repeat(50));
    console.log('API TESTS');
    console.log('='.repeat(50));
    
    let token1, token2, username1, username2;
    
    try {
        // Create two users
        const user1 = await testRegisterLogin();
        token1 = user1.token;
        username1 = user1.username;
        
        const user2 = await testRegisterLogin();
        token2 = user2.token;
        username2 = user2.username;
        
        // Upload keys for both users
        await testKeysUpload(token1, username1);
        await testKeysUpload(token2, username2);
        
        // List devices
        await testListDevices(token1);
        await testListDevices(token2);
        
        // Claim keys
        await testClaimKeys(token1, username2);
        await testClaimKeys(token2, username1);
        
        console.log('\n[OK] All API tests passed!');
        return true;
    } catch (err) {
        console.error(`\n[FAIL] API test failed: ${err.message}`);
        if (err.response) {
            console.error(`  Status: ${err.response.status}`);
            console.error(`  Data: ${JSON.stringify(err.response.data)}`);
        }
        return false;
    }
}

export { runApiTests };

if (import.meta.url === `file://${process.argv[1]}`) {
    runApiTests().then(ok => process.exit(ok ? 0 : 1));
}
