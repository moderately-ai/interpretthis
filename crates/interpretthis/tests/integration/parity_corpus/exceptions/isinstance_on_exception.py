# Pins: isinstance(e, ValueError) and isinstance(e, Exception)
# both work — the exception hierarchy walks for built-in types.
try:
    raise ValueError('msg')
except Exception as e:
    print(isinstance(e, ValueError))
    print(isinstance(e, Exception))
    print(isinstance(e, TypeError))
