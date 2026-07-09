# URL-safe alphabet replaces + and / with - and _ respectively.
# Pins the URL_SAFE engine routing.
import base64
# Input chosen to produce + or / under standard alphabet.
# b'\xfb\xff' produces "+/8=" with the standard alphabet.
data = b"\xfb\xff"
print(base64.b64encode(data))
print(base64.urlsafe_b64encode(data))
# Round-trip
print(base64.urlsafe_b64decode(base64.urlsafe_b64encode(b"hello")))
