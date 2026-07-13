# sha512 hexdigest + digest + name attribute.
import hashlib
h = hashlib.sha512(b"hello")
print(h.hexdigest())
print(len(h.digest()))         # 64 bytes
print(h.name)
print(h.digest_size)           # 64
print(h.block_size)            # 128
