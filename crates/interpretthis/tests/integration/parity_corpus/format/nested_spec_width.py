# A format spec may itself contain replacement fields (`{value:{width}}`); the
# nested field supplies the width, and numbers default to right alignment.
w = 5
print(f"{42:{w}}")
print(f"{42:{w}d}")
print(f"{3.14:{w}.1f}")
prec = 2
print(f"{3.14159:.{prec}f}")
print(f"{'hi':{w}}")            # strings default to left alignment
print(f"{7:{w}<{w}}")           # explicit fill/align still honoured
