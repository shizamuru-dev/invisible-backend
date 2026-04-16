import { runApiTests } from './api_test.js';
import { runWsTests } from './ws_test.js';
import { runE2EETests } from './e2ee_test.js';

async function main() {
    console.log('╔══════════════════════════════════════════════════╗');
    console.log('║     Invisible Backend - Test Suite              ║');
    console.log('╚══════════════════════════════════════════════════╝');
    
    let allPassed = true;
    
    allPassed = await runApiTests() && allPassed;
    allPassed = await runWsTests() && allPassed;
    allPassed = await runE2EETests() && allPassed;
    
    console.log('\n' + '='.repeat(50));
    if (allPassed) {
        console.log('[SUCCESS] All tests passed!');
    } else {
        console.log('[FAILURE] Some tests failed!');
    }
    console.log('='.repeat(50));
    
    process.exit(allPassed ? 0 : 1);
}

main();
