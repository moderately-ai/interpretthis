# json.dumps on a positive integer renders as bare decimal.
import json
print(json.dumps(42))
