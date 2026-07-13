# math.sqrt of 2 prints its full repr; both engines must round-trip the
# same f64 bit pattern.
import math

print(math.sqrt(2))
