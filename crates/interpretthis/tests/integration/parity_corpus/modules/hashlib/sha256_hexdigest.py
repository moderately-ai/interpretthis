# hashlib.sha256(data).hexdigest() — the canonical use of hashlib.
# Pins Value::HashDigest + dispatch_hash_method.
import hashlib
print(hashlib.sha256(b"hello").hexdigest())
print(hashlib.sha256(b"").hexdigest())
print(hashlib.sha256(b"abcdefghijklmnopqrstuvwxyz").hexdigest())
