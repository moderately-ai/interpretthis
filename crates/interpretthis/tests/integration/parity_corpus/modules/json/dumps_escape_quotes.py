# Embedded double quotes must be escaped as `\"` inside the JSON string.
import json
print(json.dumps('say "hello"'))
