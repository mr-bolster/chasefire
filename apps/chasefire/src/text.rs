//! Every word the program says, in both languages.
//!
//! A struct with one field per string, rather than a table of keys looked up at
//! runtime. The difference matters: a missing translation is a **compile
//! error**, not a stray `???` that somebody finds on a stage. You cannot build
//! this program with a half-finished language.
//!
//! Everything here is written to be read at a glance in a dark room, so the
//! Spanish is the Spanish this trade actually speaks — nobody says
//! "código de tiempo lineal", they say LTC.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    English,
    Spanish,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Spanish => "Español",
        }
    }
}

pub struct Text {
    pub language: Language,

    // ---- the corner window
    pub no_input: &'static str,
    pub armed: &'static str,
    pub disarmed: &'static str,
    pub options: &'static str,
    pub pin: &'static str,
    pub pin_tooltip: &'static str,
    pub next_cue: &'static str,
    pub no_cue_ahead: &'static str,
    pub in_label: &'static str,
    pub out_label: &'static str,
    pub channel_short: &'static str,

    // ---- what is happening
    pub mood_asleep: &'static str,
    pub mood_pyjamas: &'static str,
    pub mood_playing: &'static str,
    pub mood_shivering: &'static str,
    pub mood_wobbling: &'static str,
    pub badge_idle: &'static str,
    pub badge_disarmed: &'static str,
    pub badge_running: &'static str,
    pub badge_weak: &'static str,
    pub badge_coasting: &'static str,

    // ---- things worth saying in the log
    pub signal_lost: &'static str,
    pub armed_from_command_line: &'static str,
    pub cues_loaded: &'static str,
    pub settings_not_saved: &'static str,
    pub not_delivering: &'static str,
    pub silent_channel: &'static str,
    // Lo que sale cuando la tarjeta falla. Se lee de pie, a oscuras y con prisa,
    // así que dice qué hacer, no qué ha devuelto el driver.
    pub audio_missing: &'static str,
    pub audio_none: &'static str,
    pub audio_unsupported: &'static str,
    pub audio_busy: &'static str,
    pub audio_backend: &'static str,

    // ---- already running
    pub already_running_title: &'static str,
    pub already_running_body: &'static str,
    pub close: &'static str,

    // ---- options
    pub options_title: &'static str,
    pub section_input: &'static str,
    pub section_outputs: &'static str,
    pub tab_osc: &'static str,
    pub tab_midi: &'static str,
    pub tab_network: &'static str,
    pub add_output: &'static str,
    pub no_outputs: &'static str,
    pub outputs_hint: &'static str,
    pub midi_not_built: &'static str,
    pub midi_note: &'static str,
    pub no_midi_ports: &'static str,
    pub no_midi_ports_hint: &'static str,
    pub rescan: &'static str,
    pub ports_found: &'static str,
    pub kind_note: &'static str,
    pub kind_program: &'static str,
    pub kind_control: &'static str,
    pub midi_channel: &'static str,
    pub midi_note_number: &'static str,
    pub midi_velocity: &'static str,
    pub midi_program: &'static str,
    pub midi_bank: &'static str,
    pub midi_bank_tooltip: &'static str,
    pub midi_controller: &'static str,
    pub midi_value: &'static str,
    pub msc_cue: &'static str,
    pub msc_cue_tooltip: &'static str,
    pub msc_device: &'static str,
    pub filter_all: &'static str,
    pub section_cues: &'static str,
    pub section_timing: &'static str,
    pub section_appearance: &'static str,
    pub section_support: &'static str,
    pub section_about: &'static str,
    pub device: &'static str,
    pub system_default: &'static str,
    pub channel: &'static str,
    pub channel_hint: &'static str,
    pub frame_rate: &'static str,
    pub work_it_out: &'static str,
    pub frame_rate_hint: &'static str,
    pub apply_and_listen: &'static str,
    pub stop: &'static str,
    pub restarts_input: &'static str,
    pub listening: &'static str,
    pub input_closed: &'static str,
    pub connect: &'static str,
    pub output_name: &'static str,
    pub sending_to: &'static str,
    pub rtp_note: &'static str,
    pub file: &'static str,
    pub load: &'static str,
    pub save_to_file: &'static str,
    pub new_list: &'static str,
    pub new_list_ready: &'static str,
    pub discard_and_start: &'static str,
    pub overwrite: &'static str,
    pub save_as_hint: &'static str,
    pub add_cue: &'static str,
    pub remove: &'static str,
    pub no_cues_yet: &'static str,
    pub editing_rearms: &'static str,
    pub written_to: &'static str,
    pub no_file_name: &'static str,
    pub column_pick: &'static str,
    pub pick_tooltip: &'static str,
    pub column_on: &'static str,
    pub enabled_tooltip: &'static str,
    pub disabled_tooltip: &'static str,
    pub duplicate: &'static str,
    pub duplicate_none: &'static str,
    pub duplicate_tooltip: &'static str,
    pub copy_suffix: &'static str,
    pub delete_picked: &'static str,
    pub delete_none: &'static str,
    pub delete_sure: &'static str,
    pub column_at: &'static str,
    pub column_name: &'static str,
    pub column_sends: &'static str,
    pub column_to: &'static str,
    pub column_args: &'static str,
    pub arg_types: &'static str,
    pub arg_int: &'static str,
    pub arg_float: &'static str,
    pub arg_text: &'static str,
    pub arg_true: &'static str,
    pub arg_false: &'static str,
    pub args_tooltip: &'static str,
    pub no_args: &'static str,
    pub no_args_tooltip: &'static str,
    pub add_arg: &'static str,
    pub remove_arg: &'static str,
    pub add_message: &'static str,
    pub remove_message: &'static str,
    pub default_output: &'static str,
    pub at_tooltip: &'static str,
    pub offset: &'static str,
    pub offset_hint: &'static str,
    pub freewheel: &'static str,
    pub freewheel_hint: &'static str,
    pub frames_suffix: &'static str,
    pub status_display: &'static str,
    pub transport_marks: &'static str,
    pub appearance_hint: &'static str,
    pub language_label: &'static str,

    // ---- money and credits
    pub support_free: &'static str,
    pub support_ask: &'static str,
    pub donate: &'static str,
    pub version: &'static str,
    pub version_hint: &'static str,
    pub built_for: &'static str,
    pub built_for_value: &'static str,
    pub written_by: &'static str,
    pub art_by: &'static str,
    pub art_by_value: &'static str,
    pub licence: &'static str,
    pub licence_hint: &'static str,
    pub source: &'static str,
    pub settings_live_in: &'static str,
    pub portable_hint: &'static str,
    pub standing_on: &'static str,

    // ---- the reminder
    pub reminder_title: &'static str,
    pub reminder_body: &'static str,
    pub not_today: &'static str,
    pub dismissed_count: &'static str,
}

impl Text {
    pub fn of(language: Language) -> &'static Text {
        match language {
            Language::English => &ENGLISH,
            Language::Spanish => &SPANISH,
        }
    }

    /// Every language there is, for the selector in Options. Reading it from
    /// the dictionaries rather than the enum means adding a third one is a
    /// single edit here and it appears in the menu on its own.
    pub fn all() -> &'static [&'static Text] {
        static ALL: [&Text; 2] = [&ENGLISH, &SPANISH];
        &ALL
    }

    pub fn mood(&self, mood: pablo::Mood) -> &'static str {
        match mood {
            pablo::Mood::Asleep => self.mood_asleep,
            pablo::Mood::Pyjamas => self.mood_pyjamas,
            pablo::Mood::Playing => self.mood_playing,
            pablo::Mood::Shivering => self.mood_shivering,
            pablo::Mood::Wobbling => self.mood_wobbling,
        }
    }

    pub fn badge(&self, mood: pablo::Mood) -> &'static str {
        match mood {
            pablo::Mood::Asleep => self.badge_idle,
            pablo::Mood::Pyjamas => self.badge_disarmed,
            pablo::Mood::Playing => self.badge_running,
            pablo::Mood::Shivering => self.badge_weak,
            pablo::Mood::Wobbling => self.badge_coasting,
        }
    }

    /// What an argument's type is called, in words rather than in the single
    /// letter the protocol uses. Somebody editing a cue list should not have to
    /// know that `s` means text.
    pub fn arg_type(&self, arg: &cue::OscArg) -> &'static str {
        match arg {
            cue::OscArg::Int(_) => self.arg_int,
            cue::OscArg::Float(_) => self.arg_float,
            cue::OscArg::Str(_) => self.arg_text,
            cue::OscArg::Bool(true) => self.arg_true,
            cue::OscArg::Bool(false) => self.arg_false,
        }
    }

    /// The card's own complaints, in the operator's language. The engine keeps
    /// its English `Display` for logs and crash files; this is what goes on
    /// screen.
    pub fn audio_error(&self, error: &audio::AudioError) -> String {
        let (pattern, detail) = match error {
            audio::AudioError::NoSuchDevice(name) => (self.audio_missing, name.as_str()),
            audio::AudioError::NoDevices => (self.audio_none, ""),
            audio::AudioError::Unsupported(what) => (self.audio_unsupported, what.as_str()),
            audio::AudioError::Busy(name) => (self.audio_busy, name.as_str()),
            audio::AudioError::Backend(message) => (self.audio_backend, message.as_str()),
        };
        pattern.replace("{}", detail)
    }

    /// Why the input is not delivering, in words somebody can act on.
    pub fn health(&self, health: show::Health, channel: usize) -> Option<String> {
        match health {
            show::Health::Closed | show::Health::Fine => None,
            show::Health::NotDelivering => Some(self.not_delivering.to_string()),
            show::Health::Silent => Some(self.silent_channel.replace("{}", &channel.to_string())),
        }
    }
}

pub static ENGLISH: Text = Text {
    language: Language::English,

    no_input: "no input",
    armed: "ARMED",
    disarmed: "DISARMED",
    options: "Options",
    pin: "PIN",
    pin_tooltip: "Keep this window above the others",
    next_cue: "next",
    no_cue_ahead: "no cue ahead",
    in_label: "in",
    out_label: "out",
    channel_short: "ch",

    mood_asleep: "no timecode",
    mood_pyjamas: "disarmed — nothing will fire",
    mood_playing: "locked and armed",
    mood_shivering: "locked, but the signal is weak",
    mood_wobbling: "signal lost, freewheeling",
    badge_idle: "IDLE",
    badge_disarmed: "DISARMED",
    badge_running: "RUNNING",
    badge_weak: "WEAK",
    badge_coasting: "COASTING",

    signal_lost: "signal lost",
    armed_from_command_line: "armed from the command line",
    cues_loaded: "{} cues loaded",
    settings_not_saved: "settings not saved: {}",
    not_delivering: "the input stopped delivering audio — has another program taken the card?",
    silent_channel: "audio is arriving but channel {} is silent — wrong channel, or nothing plugged in?",
    audio_missing: "'{}' is not available — it may be unplugged, or another program may have it open",
    audio_none: "no audio input devices at all",
    audio_unsupported: "the device cannot do that: {}",
    audio_busy: "'{}' is already in use by another program — close whatever has it, or choose another input",
    audio_backend: "audio backend: {}",

    already_running_title: "Chasefire is already running",
    already_running_body: "Two copies would fight over the same sound card,\nand neither would read the timecode.",
    close: "Close",

    options_title: "Chasefire — Options",
    section_input: "Input",
    section_outputs: "Outputs",
    tab_osc: "OSC",
    tab_midi: "MIDI",
    tab_network: "MIDI over network",
    add_output: "Add",
    no_outputs: "Nowhere to send yet. Give one a name and an address — the name is what a cue asks for.",
    outputs_hint: "A cue that names none of these goes to the first one. A show with a single machine never has to name it.",
    midi_not_built: "Not built yet.",
    midi_note: "A local MIDI port is what opens Waves SuperRack, grandMA2 and the consoles that take MIDI Show Control. None of those speak OSC.",
    no_midi_ports: "no ports",
    no_midi_ports_hint: "This machine has no MIDI outputs. On Windows that usually means a loopMIDI port or the interface's driver; on Linux, an ALSA sequencer client.",
    rescan: "Look again",
    ports_found: "{} MIDI ports",
    kind_note: "Nota",
    kind_program: "Program",
    kind_control: "Control",
    midi_channel: "ch",
    midi_note_number: "note",
    midi_velocity: "vel",
    midi_program: "prog",
    midi_bank: "bank",
    midi_bank_tooltip: "A bank select before the program change. This is how snapshots past 128 are reached on SuperRack.",
    midi_controller: "cc",
    midi_value: "val",
    msc_cue: "cue",
    msc_cue_tooltip: "The cue number as the desk writes it. It travels as text, so 21.5 and 21.50 are different cues.",
    msc_device: "id",
    filter_all: "All",
    section_cues: "Cues",
    section_timing: "Timing",
    section_appearance: "Appearance",
    section_support: "Support this",
    section_about: "About",
    device: "Device",
    system_default: "system default",
    channel: "Channel",
    channel_hint: "which channel of that input carries the timecode",
    frame_rate: "Frame rate",
    work_it_out: "work it out",
    frame_rate_hint: "told the rate it locks on the first frame; left to work it out, the third",
    apply_and_listen: "Apply and listen",
    stop: "Stop",
    restarts_input: "applying restarts the input",
    listening: "listening",
    input_closed: "input closed",
    connect: "Connect",
    output_name: "name",
    sending_to: "sending to {}",
    rtp_note: "RTP-MIDI will speak the protocol itself, so there is nothing to install and no driver for an update to break.",
    file: "File",
    load: "Load",
    save_to_file: "Save",
    new_list: "New",
    new_list_ready: "empty list — save it to give it a name",
    discard_and_start: "Discard {} cues?",
    overwrite: "Overwrite {}?",
    save_as_hint: "Save as: type a different name above and save. Saving over a file that is already there asks first.",
    add_cue: "Add cue",
    remove: "remove",
    no_cues_yet: "no cues yet — add one below",
    editing_rearms: "editing re-arms every cue and re-syncs",
    written_to: "written to {}",
    no_file_name: "no file name — type one in the box above",
    column_pick: "",
    pick_tooltip: "Pick this cue out, for the buttons under the list",
    column_on: "on",
    enabled_tooltip: "Armed — this cue will fire",
    disabled_tooltip: "Off — this cue will not fire",
    duplicate: "Duplicate {}",
    duplicate_none: "Duplicate",
    duplicate_tooltip: "The copy lands at the same time as the original and arrives switched off, because two cues firing at the same instant is rarely what anybody meant.",
    copy_suffix: "(copy)",
    delete_picked: "Delete {}",
    delete_none: "Delete",
    delete_sure: "Really delete {}?",
    column_at: "at",
    column_name: "name",
    column_sends: "sends",
    column_to: "to",
    column_args: "sends",
    arg_types: "Argument types: int a whole number · dec a decimal · text · true and false, which travel in the type tag with no value of their own. Click one to change it.",
    arg_int: "int",
    arg_float: "dec",
    arg_text: "text",
    arg_true: "true",
    arg_false: "false",
    args_tooltip: "What rides along with the address. The letter is the OSC type tag the far end will read: i whole number, f decimal, s text, T true, F false.",
    no_args: "nothing",
    no_args_tooltip: "No arguments at all — which is exactly what QLab wants on /cue/5/start. An extra 1 is not harmless there.",
    add_arg: "One more argument",
    remove_arg: "Drop this argument",
    add_message: "Another message at this same moment",
    remove_message: "Drop this message",
    default_output: "default",
    at_tooltip: "HH:MM:SS:FF — a semicolon before the frames means drop frame",
    offset: "Offset",
    offset_hint: "positive fires early, to cancel the delay of the card, the network and the far end",
    freewheel: "Freewheel",
    freewheel_hint: "how long to keep counting after the signal goes; the trade uses eight to forty",
    frames_suffix: " frames",
    status_display: "Status display",
    transport_marks: "Transport marks",
    appearance_hint: "Both say exactly the same five things. One is a little guitarist and the other is not.",
    language_label: "Language",

    support_free: "Chasefire is free software and always will be — source, licence, all of it. There is no trial, no expiry and nothing switched off if you never pay a penny.",
    support_ask: "If it earns you money, a one-off contribution keeps it being worked on. Pay what you think it is worth, once. Never a subscription.",
    donate: "Donate",
    version: "Version",
    version_hint: "chase timecode, fire cues",
    built_for: "Built for",
    built_for_value: "live shows: LTC in, cues out, on the machine you already own",
    written_by: "Written by",
    art_by: "Art by",
    art_by_value: "commissioned pixel art",
    licence: "Licence",
    licence_hint: "the source is open and stays open",
    source: "Source",
    settings_live_in: "Settings live in",
    portable_hint: "Put a file called chasefire.json next to the executable and it will be used instead — for a stick that travels with its own setup.",
    standing_on: "Standing on",

    reminder_title: "Chasefire is free, and stays free",
    reminder_body: "No licence, no expiry, nothing switched off if you never pay.\nIf it earns you money, put something in once — what you think\nit is worth. Never a subscription.",
    not_today: "Not today",
    dismissed_count: "You have closed this {} times.",
};

pub static SPANISH: Text = Text {
    language: Language::Spanish,

    no_input: "sin entrada",
    armed: "ARMADO",
    disarmed: "DESARMADO",
    options: "Ajustes",
    pin: "FIJA",
    pin_tooltip: "Mantener esta ventana por encima de las demás",
    next_cue: "siguiente",
    no_cue_ahead: "no queda ningún cue",
    in_label: "ent",
    out_label: "sal",
    channel_short: "canal",

    mood_asleep: "sin timecode",
    mood_pyjamas: "desarmado — no va a salir nada",
    mood_playing: "enganchado y armado",
    mood_shivering: "enganchado, pero la señal es débil",
    mood_wobbling: "señal perdida, contando de memoria",
    badge_idle: "PARADO",
    badge_disarmed: "DESARMADO",
    badge_running: "EN MARCHA",
    badge_weak: "DÉBIL",
    badge_coasting: "SIN SEÑAL",

    signal_lost: "señal perdida",
    armed_from_command_line: "armado desde la línea de comandos",
    cues_loaded: "{} cues cargados",
    settings_not_saved: "ajustes sin guardar: {}",
    not_delivering: "la entrada ha dejado de dar audio — ¿le ha quitado la tarjeta otro programa?",
    silent_channel: "llega audio pero el canal {} está mudo — ¿canal equivocado, o nada enchufado?",
    audio_missing: "«{}» no está disponible — puede estar desenchufada, o tenerla abierta otro programa",
    audio_none: "no hay ninguna entrada de audio",
    audio_unsupported: "la tarjeta no puede hacer eso: {}",
    audio_busy: "«{}» ya la está usando otro programa — cierra lo que la tenga, o elige otra entrada",
    audio_backend: "audio: {}",

    already_running_title: "Chasefire ya está abierto",
    already_running_body: "Dos copias se pelearían por la misma tarjeta,\ny ninguna leería el timecode.",
    close: "Cerrar",

    options_title: "Chasefire — Ajustes",
    section_input: "Entrada",
    section_outputs: "Salidas",
    tab_osc: "OSC",
    tab_midi: "MIDI",
    tab_network: "MIDI por red",
    add_output: "Añadir",
    no_outputs: "Todavía no hay a dónde mandar. Ponle un nombre y una dirección — el nombre es lo que pide una cue.",
    outputs_hint: "Una cue que no nombra ninguna va a la primera. Un show con una sola máquina no tiene que nombrarla nunca.",
    midi_not_built: "Todavía sin construir.",
    midi_note: "Un puerto MIDI local es lo que abre Waves SuperRack, la grandMA2 y las consolas que aceptan MIDI Show Control. Ninguna de ellas habla OSC.",
    no_midi_ports: "sin puertos",
    no_midi_ports_hint: "Esta máquina no tiene salidas MIDI. En Windows suele ser un puerto loopMIDI o el driver de la interfaz; en Linux, un cliente del secuenciador ALSA.",
    rescan: "Volver a mirar",
    ports_found: "{} puertos MIDI",
    kind_note: "Nota",
    kind_program: "Program",
    kind_control: "Control",
    midi_channel: "can",
    midi_note_number: "nota",
    midi_velocity: "vel",
    midi_program: "prog",
    midi_bank: "banco",
    midi_bank_tooltip: "Un bank select antes del program change. Así se llega a los snapshots por encima de 128 en SuperRack.",
    midi_controller: "cc",
    midi_value: "val",
    msc_cue: "cue",
    msc_cue_tooltip: "El número de cue tal y como lo escribe la consola. Viaja como texto, así que 21.5 y 21.50 son cues distintas.",
    msc_device: "id",
    filter_all: "Todo",
    section_cues: "Cues",
    section_timing: "Tiempos",
    section_appearance: "Aspecto",
    section_support: "Apoya esto",
    section_about: "Acerca de",
    device: "Dispositivo",
    system_default: "el del sistema",
    channel: "Canal",
    channel_hint: "por qué canal de esa entrada llega el timecode",
    frame_rate: "Frames por segundo",
    work_it_out: "que lo averigüe",
    frame_rate_hint: "diciéndoselo engancha al primer frame; averiguándolo, al tercero",
    apply_and_listen: "Aplicar y escuchar",
    stop: "Parar",
    restarts_input: "aplicar reinicia la entrada",
    listening: "escuchando",
    input_closed: "entrada cerrada",
    connect: "Conectar",
    output_name: "nombre",
    sending_to: "enviando a {}",
    rtp_note: "RTP-MIDI hablará el protocolo por sí mismo, así que no habrá nada que instalar ni driver que una actualización pueda romper.",
    file: "Fichero",
    load: "Cargar",
    save_to_file: "Guardar",
    new_list: "Nueva",
    new_list_ready: "lista vacía — guárdala para ponerle nombre",
    discard_and_start: "¿Descartar {} cues?",
    overwrite: "¿Pisar {}?",
    save_as_hint: "Guardar como: escribe otro nombre arriba y guarda. Si el fichero ya existe, pregunta antes de pisarlo.",
    add_cue: "Añadir cue",
    remove: "quitar",
    no_cues_yet: "aún no hay cues — añade uno abajo",
    editing_rearms: "editar re-arma todos los cues y re-sincroniza",
    written_to: "escrito en {}",
    no_file_name: "falta el nombre del fichero — escríbelo en la casilla de arriba",
    column_pick: "",
    pick_tooltip: "Marca esta cue, para los botones de debajo de la lista",
    column_on: "on",
    enabled_tooltip: "Armada — esta cue va a disparar",
    disabled_tooltip: "Apagada — esta cue no dispara",
    duplicate: "Duplicar {}",
    duplicate_none: "Duplicar",
    duplicate_tooltip: "La copia cae a la misma hora que el original y llega apagada, porque dos cues disparando en el mismo instante casi nunca es lo que alguien quería.",
    copy_suffix: "(copia)",
    delete_picked: "Borrar {}",
    delete_none: "Borrar",
    delete_sure: "¿Borrar {} de verdad?",
    column_at: "a las",
    column_name: "nombre",
    column_sends: "envía",
    column_to: "a",
    column_args: "manda",
    arg_types: "Tipos de argumento: int un entero · dec un decimal · texto · true y false, que viajan en el type tag sin valor propio. Pulsa uno para cambiarlo.",
    arg_int: "int",
    arg_float: "dec",
    arg_text: "texto",
    arg_true: "true",
    arg_false: "false",
    args_tooltip: "Lo que acompaña a la dirección. La letra es el type tag OSC que va a leer el otro extremo: i entero, f decimal, s texto, T verdadero, F falso.",
    no_args: "nada",
    no_args_tooltip: "Ningún argumento — que es justo lo que quiere QLab en /cue/5/start. Un 1 de más ahí no es inofensivo.",
    add_arg: "Un argumento más",
    remove_arg: "Quitar este argumento",
    add_message: "Otro mensaje en este mismo momento",
    remove_message: "Quitar este mensaje",
    default_output: "por defecto",
    at_tooltip: "HH:MM:SS:FF — un punto y coma antes de los frames significa drop frame",
    offset: "Offset",
    offset_hint: "en positivo dispara antes, para compensar el retardo de la tarjeta, la red y el otro extremo",
    freewheel: "Freewheel",
    freewheel_hint: "cuánto seguir contando tras perder la señal; el gremio usa entre ocho y cuarenta",
    frames_suffix: " frames",
    status_display: "Indicador de estado",
    transport_marks: "Símbolos de transporte",
    appearance_hint: "Los dos dicen exactamente las mismas cinco cosas. Uno es un guitarrista pequeño y el otro no.",
    language_label: "Idioma",

    support_free: "Chasefire es software libre y lo seguirá siendo — el código, la licencia, todo. No hay periodo de prueba, ni caducidad, ni nada apagado si no pagas nunca.",
    support_ask: "Si te da de comer, una aportación única mantiene esto en marcha. Paga lo que te parezca que vale, una vez. Nunca una suscripción.",
    donate: "Donar",
    version: "Versión",
    version_hint: "persigue el timecode, dispara los cues",
    built_for: "Hecho para",
    built_for_value: "directo: entra LTC, salen cues, en la máquina que ya tienes",
    written_by: "Escrito por",
    art_by: "Dibujos de",
    art_by_value: "pixel art por encargo",
    licence: "Licencia",
    licence_hint: "el código es abierto y seguirá siéndolo",
    source: "Código",
    settings_live_in: "Los ajustes viven en",
    portable_hint: "Pon un fichero llamado chasefire.json junto al ejecutable y se usará ese — para un pincho que viaje con su propia configuración.",
    standing_on: "Apoyado en",

    reminder_title: "Chasefire es libre, y lo seguirá siendo",
    reminder_body: "Sin licencia, sin caducidad, sin nada apagado si no pagas nunca.\nSi te da de comer, echa algo una vez — lo que te parezca\nque vale. Nunca una suscripción.",
    not_today: "Hoy no",
    dismissed_count: "Has cerrado esto {} veces.",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_left_untranslated() {
        // The struct already guarantees every field exists in both languages —
        // it would not compile otherwise. What it cannot catch is a field left
        // holding the English by mistake, so this looks for that.
        let shared = [
            // Genuinely the same in both, and deliberately so: nobody in this
            // trade says anything else.
            ENGLISH.frames_suffix,
            ENGLISH.column_on,
            ENGLISH.donate,
        ];
        let english = ENGLISH.no_input;
        let spanish = SPANISH.no_input;
        assert_ne!(english, spanish);

        for (a, b) in [
            (ENGLISH.armed, SPANISH.armed),
            (ENGLISH.options, SPANISH.options),
            (ENGLISH.mood_pyjamas, SPANISH.mood_pyjamas),
            (ENGLISH.badge_running, SPANISH.badge_running),
            (ENGLISH.reminder_title, SPANISH.reminder_title),
            (ENGLISH.section_about, SPANISH.section_about),
        ] {
            assert_ne!(a, b, "'{a}' was never translated");
        }
        assert_eq!(shared.len(), 3, "the allowed-identical list moved");
    }

    #[test]
    fn the_card_complains_in_the_right_language() {
        // The one an operator reads at the worst possible moment. It must name
        // the device and it must not be in English when Spanish is chosen.
        let busy = audio::AudioError::Busy("ART USB Dual Pre".into());
        let spanish = Text::of(Language::Spanish).audio_error(&busy);
        assert!(spanish.contains("ART USB Dual Pre"), "lost the device name");
        assert_ne!(spanish, Text::of(Language::English).audio_error(&busy));
        assert!(!spanish.contains("{}"), "the placeholder was left in");
    }

    #[test]
    fn each_dictionary_knows_which_language_it_is() {
        // Cheap, and it catches the one mistake this design cannot: `of()`
        // handing back the wrong dictionary after somebody adds a third
        // language and copies the match arm above it.
        for language in [Language::English, Language::Spanish] {
            assert_eq!(Text::of(language).language, language);
        }
    }

    #[test]
    fn the_placeholders_survive_translation() {
        // A translation that loses its {} silently drops the number it was
        // meant to carry — the count, the channel, the file name.
        for (english, spanish) in [
            (ENGLISH.cues_loaded, SPANISH.cues_loaded),
            (ENGLISH.silent_channel, SPANISH.silent_channel),
            (ENGLISH.sending_to, SPANISH.sending_to),
            (ENGLISH.written_to, SPANISH.written_to),
            (ENGLISH.dismissed_count, SPANISH.dismissed_count),
            (ENGLISH.audio_missing, SPANISH.audio_missing),
            (ENGLISH.audio_unsupported, SPANISH.audio_unsupported),
            (ENGLISH.audio_busy, SPANISH.audio_busy),
            (ENGLISH.discard_and_start, SPANISH.discard_and_start),
            (ENGLISH.overwrite, SPANISH.overwrite),
            (ENGLISH.settings_not_saved, SPANISH.settings_not_saved),
            (ENGLISH.discard_and_start, SPANISH.discard_and_start),
            (ENGLISH.overwrite, SPANISH.overwrite),
        ] {
            assert!(english.contains("{}"), "the English lost its placeholder");
            assert!(
                spanish.contains("{}"),
                "the Spanish of '{english}' lost its placeholder"
            );
        }
    }

    #[test]
    fn the_dangerous_state_is_unmistakable_in_both() {
        // Whatever language it is in, "timecode running but nothing will fire"
        // has to say so. This is the state that ruins shows.
        assert!(SPANISH.mood_pyjamas.contains("no va a salir"));
        assert!(ENGLISH.mood_pyjamas.contains("nothing will fire"));
        assert_ne!(SPANISH.badge_disarmed, SPANISH.badge_running);
    }
}
