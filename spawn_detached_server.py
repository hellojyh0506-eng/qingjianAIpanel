import sys
import ctypes
from ctypes import wintypes

kernel32 = ctypes.windll.kernel32

class STARTUPINFO(ctypes.Structure):
    _fields_ = [
        ('cb', wintypes.DWORD),
        ('lpReserved', wintypes.LPWSTR),
        ('lpDesktop', wintypes.LPWSTR),
        ('lpTitle', wintypes.LPWSTR),
        ('dwX', wintypes.DWORD),
        ('dwY', wintypes.DWORD),
        ('dwXSize', wintypes.DWORD),
        ('dwYSize', wintypes.DWORD),
        ('dwXCountChars', wintypes.DWORD),
        ('dwYCountChars', wintypes.DWORD),
        ('dwFillAttribute', wintypes.DWORD),
        ('dwFlags', wintypes.DWORD),
        ('wShowWindow', wintypes.WORD),
        ('cbReserved2', wintypes.WORD),
        ('lpReserved2', ctypes.c_char_p),
        ('hStdInput', wintypes.HANDLE),
        ('hStdOutput', wintypes.HANDLE),
        ('hStdError', wintypes.HANDLE),
    ]

class PROCESS_INFORMATION(ctypes.Structure):
    _fields_ = [
        ('hProcess', wintypes.HANDLE),
        ('hThread', wintypes.HANDLE),
        ('dwProcessId', wintypes.DWORD),
        ('dwThreadId', wintypes.DWORD),
    ]

si = STARTUPINFO()
si.cb = ctypes.sizeof(STARTUPINFO)
si.lpDesktop = r"winsta0\default"

pi = PROCESS_INFORMATION()

exe_path = r"E:\Qingjian\qingjian-server.exe"
work_dir = r"E:\Qingjian"

DETACHED_PROCESS = 0x00000008
CREATE_NEW_PROCESS_GROUP = 0x00000200
flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP

ret = kernel32.CreateProcessW(
    exe_path,
    None,
    None,
    None,
    False,
    flags,
    None,
    work_dir,
    ctypes.byref(si),
    ctypes.byref(pi)
)

if not ret:
    err = kernel32.GetLastError()
    print(f"Failed to spawn detached server, error: {err}")
    sys.exit(1)

print(f"qingjian-server spawned detached on winsta0\\default! PID={pi.dwProcessId}")
kernel32.CloseHandle(pi.hThread)
kernel32.CloseHandle(pi.hProcess)
