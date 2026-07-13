# Pins: str(e) on a caught exception is just the message — no line
# info, no type prefix, no quotes. Our (at line N) stamp must NOT
# bleed into the user-visible Display.
try:
    raise ValueError('boom')
except ValueError as e:
    print(str(e))
    print(f'fstr: {e}')
    print(repr(e))
