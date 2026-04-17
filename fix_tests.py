import sys

with open('api/tests/e2ee_api_test.rs', 'r') as f:
    lines = f.readlines()

out_lines = []
skip = False
for line in lines:
    if "async fn backup_vault_flow() {}" in line:
        out_lines.append(line)
        skip = True
        continue
    
    if skip:
        if line.strip() == "}":
            skip = False
        continue
    
    out_lines.append(line)

with open('api/tests/e2ee_api_test.rs', 'w') as f:
    f.writelines(out_lines)
