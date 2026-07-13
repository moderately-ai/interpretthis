# Pins: bare `except:` catches a ZeroDivisionError.
try:
    x = 1 / 0
except:
    x = "caught"
print(x)
