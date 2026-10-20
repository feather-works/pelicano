use pelicano::raw_window_handle::RawWindowHandle;
use pelicano::{Event, Point, Task, WindowId};

#[test]
fn alca_e_evento_sintetico_montam_tarefas_e_passam_pelo_map() {
    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        X11(bool),
    }
    let alca: Task<bool> = Task::window_handle(WindowId::MAIN, |h| matches!(h, RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)));
    let evento: Task<bool> = Task::window_event(WindowId::MAIN, Event::CursorMoved(Point::new(3.0, 4.0)));
    let _: Task<Msg> = Task::batch([alca, evento]).map(Msg::X11);
}
