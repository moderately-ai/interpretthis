import base64
print(base64.b16encode(b"AB").decode())
print(base64.b16decode("4142").decode())
print(base64.b32encode(b"hi").decode())
print(base64.b32decode(base64.b32encode(b"test")).decode())
print(base64.urlsafe_b64encode(b"a?b>c").decode())
print(base64.urlsafe_b64decode(base64.urlsafe_b64encode(b"data")).decode())
