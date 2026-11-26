#!/usr/bin/env bash
# Close issues opened by wrong author and recreate under martinzhames
# Reads from /tmp/stellar_wrong_author.json
set -e

REPO="dupdab/dupdapp_stellar"

echo "Reading issues from /tmp/stellar_remaining.json..."

# Read titles into array using python
mapfile -t NUMS < <(python3 -c "
import json
d=json.load(open('/tmp/stellar_remaining.json'))
for i in sorted(d, key=lambda x: x['n']):
    print(i['n'])
")

mapfile -t TITLES < <(python3 -c "
import json
d=json.load(open('/tmp/stellar_remaining.json'))
for i in sorted(d, key=lambda x: x['n']):
    print(i['t'])
")

TOTAL=${#NUMS[@]}
echo "Processing $TOTAL issues..."

for i in "${!NUMS[@]}"; do
    NUM="${NUMS[$i]}"
    TITLE="${TITLES[$i]}"
    IDX=$((i+1))

    # Get the full body before closing
    BODY=$(gh issue view "$NUM" --repo "$REPO" --json body --jq '.body' 2>/dev/null || echo "")

    # Close the old issue
    gh issue close "$NUM" --repo "$REPO" 2>/dev/null

    # Recreate under martinzhames (currently active account)
    NEW=$(gh issue create --repo "$REPO" --title "$TITLE" --body "$BODY" 2>/dev/null)
    echo "[$IDX/$TOTAL] Closed #$NUM → Created $NEW"
    sleep 0.3
done

echo "Done. All $TOTAL issues recreated under martinzhames."
