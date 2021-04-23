use bindings::{
    Windows::Win32::Direct3D11::*, Windows::Win32::Direct3D12::*, Windows::Win32::Dxgi::*,
    Windows::Win32::SystemServices::*, Windows::Win32::WindowsAndMessaging::*, Windows::Win32::WindowsProgramming::*,
};
use windows::{Abi, Interface};

mod win32_utils {
    pub fn lo_word(word: usize) -> u32 {
        (word & 0xFFFF) as u32
    }

    pub fn hi_word(word: usize) -> u32 {
        ((word >> 16) & 0xFFFF) as u32
    }
}

fn main() -> windows::Result<()> {
    let mut window = Window::new()?;
    window.run()
}

const FRAME_COUNT: u32 = 3;

struct Window {
    handle: HWND,
    is_visible: bool,
    width: u32,
    height: u32,

    debug_controller: Option<ID3D12Debug>,
    factory: Option<IDXGIFactory4>,
    device: Option<ID3D12Device>,

    swapchain: Option<IDXGISwapChain3>,
    render_targets: [Option<ID3D12Resource>; FRAME_COUNT as usize],

    command_allocator: Option<ID3D12CommandAllocator>,
    command_queue: Option<ID3D12CommandQueue>,
    command_list: Option<ID3D12GraphicsCommandList>,

    rtv_heap: Option<ID3D12DescriptorHeap>,
    rtv_desc_size: u32,

    pipeline_state: Option<ID3D12PipelineState>,

    frame_index: u32,
    fence_event: HANDLE,
    fence: Option<ID3D12Fence>,
    fence_value: u64,
}

impl Window {
    fn new() -> windows::Result<Self> {
        Ok(Window {
            handle: HWND(0),
            is_visible: true,
            width: 720,
            height: 720,

            debug_controller: None,
            factory: None,
            device: None,

            swapchain: None,
            render_targets: Default::default(),

            command_allocator: None,
            command_queue: None,
            command_list: None,

            rtv_heap: None,
            rtv_desc_size: 0,

            pipeline_state: None,

            frame_index: 0,
            fence_event: HANDLE(0),
            fence: None,
            fence_value: 0,
        })
    }

    // Cannot be in new() because there is pointer to self as user ptr
    fn init(&mut self) -> windows::Result<()> {
        // Create the window
        unsafe {
            let instance = HINSTANCE(GetModuleHandleA(None));
            debug_assert!(instance.0 != 0);

            let wc = WNDCLASSA {
                hCursor: LoadCursorW(None, IDC_HAND),
                hInstance: instance,
                lpszClassName: PSTR(b"window\0".as_ptr() as _),

                style: WNDCLASS_STYLES::CS_HREDRAW | WNDCLASS_STYLES::CS_VREDRAW,
                lpfnWndProc: Some(Self::wndproc),
                ..Default::default()
            };

            let atom = RegisterClassA(&wc);
            debug_assert!(atom != 0);

            let handle = CreateWindowExA(
                Default::default(),
                "window",
                "Test Rust DX12",
                WINDOW_STYLE::WS_OVERLAPPEDWINDOW | WINDOW_STYLE::WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                instance,
                self as *mut _ as _,
            );

            if handle.is_null() {
                // GetLastError() wrapper
                windows::HRESULT::from_thread().ok()?;
            }
        }

        // -- DirectX12 initialization

        self.debug_controller = None;

        let mut dxgi_factory_flags: u32 = 0;

        // Enable the debug layer (requires the Graphics Tools "optional feature").
        // NOTE: Enabling the debug layer after device creation will invalidate the active device.
        unsafe {
            if D3D12GetDebugInterface(&ID3D12Debug::IID, self.debug_controller.set_abi()).is_ok() {
                self.debug_controller.as_ref().unwrap().EnableDebugLayer();
                dxgi_factory_flags = dxgi_factory_flags | DXGI_CREATE_FACTORY_DEBUG;
            }
        }

        self.factory = Some(unsafe {
            let mut res: Option<IDXGIFactory4> = None;
            CreateDXGIFactory2(dxgi_factory_flags, &IDXGIFactory4::IID, res.set_abi()).and_some(res)?
        });

        self.device = Some(unsafe {
            let mut res: Option<ID3D12Device> = None;
            D3D12CreateDevice(
                None,
                D3D_FEATURE_LEVEL::D3D_FEATURE_LEVEL_12_1,
                &ID3D12Device::IID,
                res.set_abi(),
            )
            .and_some(res)?
        });

        // Describe and create the command queue.
        let queue_desc: D3D12_COMMAND_QUEUE_DESC = Default::default();

        self.command_queue = Some(unsafe {
            let mut res: Option<ID3D12CommandQueue> = None;
            self.device.as_ref().unwrap()
                .CreateCommandQueue(&queue_desc, &ID3D12CommandQueue::IID, res.set_abi())
                .and_some(res)?
        });

        let swapchain_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: self.width,
            Height: self.height,
            Format: DXGI_FORMAT::DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                ..Default::default()
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: FRAME_COUNT,
            SwapEffect: DXGI_SWAP_EFFECT::DXGI_SWAP_EFFECT_FLIP_DISCARD,
            ..Default::default()
        };

        let swapchain1 = unsafe {
            let mut res: Option<IDXGISwapChain1> = None;
            self.factory.as_ref().unwrap()
                .CreateSwapChainForHwnd(
                    self.command_queue.as_ref().unwrap(),
                    self.handle,
                    &swapchain_desc,
                    std::ptr::null(),
                    None,
                    &mut res,
                )
                .and_some(res)?
        };

        unsafe {
            self.factory.as_ref().unwrap()
                .MakeWindowAssociation(self.handle, DXGI_MWA_NO_ALT_ENTER)
                .ok()?
        };

        self.swapchain = Some(swapchain1.cast::<IDXGISwapChain3>()?);

        let frame_index = unsafe { self.swapchain.as_ref().unwrap().GetCurrentBackBufferIndex() };

        println!("Current frame: {}", frame_index);

        let desc_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            NumDescriptors: FRAME_COUNT,
            ..Default::default()
        };

        self.rtv_heap = Some(unsafe {
            let mut res: Option<ID3D12DescriptorHeap> = None;
            self.device.as_ref().unwrap()
                .CreateDescriptorHeap(&desc_heap_desc, &ID3D12DescriptorHeap::IID, res.set_abi())
                .and_some(res)?
        });

        self.rtv_desc_size = unsafe {
            self.device.as_ref().unwrap()
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
        };

        let mut rtv_handle = unsafe { self.rtv_heap.as_ref().unwrap().GetCPUDescriptorHandleForHeapStart() };

        for i_frame in 0..FRAME_COUNT {
            self.render_targets[i_frame as usize] = Some(unsafe {
                let mut res: Option<ID3D12Resource> = None;
                self.swapchain.as_ref().unwrap()
                    .GetBuffer(i_frame, &ID3D12Resource::IID, res.set_abi())
                    .and_some(res)?
            });

            unsafe {
                self.device.as_ref().unwrap().CreateRenderTargetView(
                    self.render_targets[i_frame as usize].as_ref().unwrap(),
                    std::ptr::null(),
                    rtv_handle,
                )
            };

            rtv_handle.ptr = rtv_handle.ptr + 1 * self.rtv_desc_size as usize;
        }

        self.command_allocator = Some(unsafe {
            let mut res: Option<ID3D12CommandAllocator> = None;
            self.device.as_ref().unwrap()
                .CreateCommandAllocator(
                    D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &ID3D12CommandAllocator::IID,
                    res.set_abi(),
                )
                .and_some(res)?
        });

        self.command_list = Some(unsafe {
            let mut res: Option<ID3D12GraphicsCommandList> = None;
            self.device.as_ref().unwrap()
                .CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    self.command_allocator.as_ref().unwrap(),
                    None,
                    &ID3D12GraphicsCommandList::IID,
                    res.set_abi(),
                )
                .and_some(res)?
        });

        // Command lists are created in the recording state by default, Close() stops recording.
        unsafe { self.command_list.as_ref().unwrap().Close().ok()? };

        self.fence = Some(unsafe {
            let mut res: Option<ID3D12Fence> = None;
            self.device.as_ref().unwrap()
                .CreateFence(
                    0,
                    D3D12_FENCE_FLAGS::D3D12_FENCE_FLAG_NONE,
                    &ID3D12Fence::IID,
                    res.set_abi(),
                )
                .and_some(res)?
        });
        self.fence_value = 1;

        self.fence_event = unsafe { CreateEventA(std::ptr::null_mut(), false, false, None) };
        if self.fence_event.is_null() {
            // GetLastError() wrapper
            windows::HRESULT::from_thread().ok()?;
        }

        Ok(())
    }

    fn render(&mut self) -> windows::Result<()> {
        unsafe {
            self.command_allocator.as_ref().unwrap().Reset().ok()?;
            self.command_list.as_ref().unwrap().Reset(self.command_allocator.as_ref().unwrap(), None).ok()?;

            // Present -> RenderTarget
            let barrier = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAGS::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: D3D12_RESOURCE_TRANSITION_BARRIER_abi {
                        pResource: self.render_targets[self.frame_index as usize].as_mut().unwrap() as *mut _ as _,
                        StateBefore: D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT,
                        StateAfter: D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET,
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    },
                },
            };
            self.command_list.as_ref().unwrap().ResourceBarrier(1, &barrier);

            let mut rtv_handle = self.rtv_heap.as_ref().unwrap().GetCPUDescriptorHandleForHeapStart();
            rtv_handle.ptr = rtv_handle.ptr + (self.frame_index * self.rtv_desc_size) as usize;

            let clear_color: [f32; 4] = [0.0, 0.2, 0.4, 1.0];
            self.command_list.as_ref().unwrap()
                .ClearRenderTargetView(rtv_handle, &clear_color[0] as _, 0, std::ptr::null());

            // RenderTarget -> Present
            let barrier = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAGS::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: D3D12_RESOURCE_TRANSITION_BARRIER_abi {
                        pResource: self.render_targets[self.frame_index as usize].as_mut().unwrap() as *mut _ as _,
                        StateBefore: D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET,
                        StateAfter: D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT,
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES
                    }
                }
            };
            self.command_list().ResourceBarrier(1, &barrier);

            self.command_list.as_ref().unwrap().Close().ok()?;

            let command_list: *mut ID3D12GraphicsCommandList = self.command_list.as_mut().unwrap();
            self.command_queue.as_ref().unwrap().ExecuteCommandLists(1, command_list as _);
            self.swapchain.as_ref().unwrap().Present(1, 0).ok()?;

            let fence = self.fence_value;
            self.command_queue.as_ref().unwrap().Signal(self.fence.as_ref().unwrap(), fence).ok()?;
            self.fence_value += 1;

            if self.fence.as_ref().unwrap().GetCompletedValue() < fence {
                self.fence.as_ref().unwrap().SetEventOnCompletion(fence, self.fence_event).ok()?;
                WaitForSingleObject(self.fence_event, INFINITE);
            }

            self.frame_index = self.swapchain.as_ref().unwrap().GetCurrentBackBufferIndex();
        }

        Ok(())
    }

    fn message_handler(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            match message {
                WM_PAINT => {
                    self.render().unwrap();

                    LRESULT(0)
                }

                WM_SIZE => {
                    self.width = win32_utils::lo_word(lparam.0 as usize);
                    self.height = win32_utils::hi_word(lparam.0 as usize);
                    /*
                    if wparam.0 != SIZE_MINIMIZED as usize {
                        self.resize_swapchain_bitmap().unwrap();
                    }
                    */
                    LRESULT(0)
                }

                WM_DISPLAYCHANGE => {
                    // self.render().unwrap();
                    LRESULT(0)
                }

                WM_USER => {
                    /*
                    if self.present(0, DXGI_PRESENT_TEST).is_ok() {
                        self.dxfactory.UnregisterOcclusionStatus(self.occlusion);
                        self.occlusion = 0;
                        self.visible = true;
                    }
                    */
                    LRESULT(0)
                }

                WM_ACTIVATE => {
                    let is_active = win32_utils::lo_word(wparam.0) != WA_INACTIVE;
                    self.is_visible = is_active;
                    LRESULT(0)
                }

                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcA(self.handle, message, wparam, lparam),
            }
        }
    }

    fn run(&mut self) -> windows::Result<()> {
        self.init()?;

        unsafe {
            let mut message = MSG::default();

            loop {
                if self.is_visible {
                    // self.render()?;

                    while PeekMessageA(&mut message, None, 0, 0, PEEK_MESSAGE_REMOVE_TYPE::PM_REMOVE).into() {
                        if message.message == WM_QUIT {
                            return Ok(());
                        }
                        DispatchMessageA(&message);
                    }
                } else {
                    GetMessageA(&mut message, None, 0, 0);

                    if message.message == WM_QUIT {
                        return Ok(());
                    }

                    DispatchMessageA(&message);
                }
            }
        }
    }

    extern "system" fn wndproc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            if message == WM_NCCREATE {
                let cs = lparam.0 as *const CREATESTRUCTA;
                let this = (*cs).lpCreateParams as *mut Self;
                (*this).handle = window;

                SetWindowLongPtrA(window, WINDOW_LONG_PTR_INDEX::GWLP_USERDATA, this as _);
            } else {
                let this = GetWindowLongPtrA(window, WINDOW_LONG_PTR_INDEX::GWLP_USERDATA) as *mut Self;

                if !this.is_null() {
                    return (*this).message_handler(message, wparam, lparam);
                }
            }

            DefWindowProcA(window, message, wparam, lparam)
        }
    }
}
