fn main() {
    windows::build!(
        Windows::Win32::Direct3D11::{D3D_FEATURE_LEVEL},
        Windows::Win32::Direct3D12::*,
        Windows::Win32::Dxgi::*,

        Windows::Win32::SystemServices::{
            CreateEventA, GetModuleHandleA, WaitForSingleObject,
            DXGI_STATUS_OCCLUDED, HINSTANCE, LRESULT,
            PSTR,
        },

        Windows::Win32::WindowsAndMessaging::{
            CreateWindowExA, DefWindowProcA, DispatchMessageA, GetMessageA, PeekMessageA,
            PostQuitMessage, RegisterClassA, CREATESTRUCTA, HWND, LPARAM, MINMAXINFO, MSG, WNDCLASSA,
            WPARAM, LoadCursorW, IDC_ARROW, SIZE_MINIMIZED, WM_DESTROY, WM_ACTIVATE, WM_DISPLAYCHANGE,
            WM_NCCREATE, WM_PAINT, WM_QUIT, WM_SIZE, WM_USER, WNDCLASS_STYLES, WA_INACTIVE,
            CW_USEDEFAULT, IDC_HAND, SetWindowLongA, SetWindowLongPtrA, GetWindowLongA, GetWindowLongPtrA,
        },

        Windows::Win32::WindowsProgramming::{
            GetLocalTime, QueryPerformanceCounter, QueryPerformanceFrequency,
            INFINITE
        },
    );
}
