import datetime as dt

# time.fromisoformat parses HH, HH:MM, HH:MM:SS, and HH:MM:SS.ffffff forms.
print(dt.time.fromisoformat("14:30:00"))
print(dt.time.fromisoformat("14:30"))
print(dt.time.fromisoformat("14"))
print(dt.time.fromisoformat("14:30:00.123456"))
print(dt.time.fromisoformat("14:30:00.5"))
print(dt.time.fromisoformat("23:59:59.999999"))
print(dt.time.fromisoformat("00:00:00"))
print(dt.time.fromisoformat("09:05:03").hour)
print(dt.time.fromisoformat("09:05:03").minute)
print(dt.time.fromisoformat("09:05:03.000042").microsecond)
