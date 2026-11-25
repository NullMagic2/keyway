//! Module: keyway_settings::i18n
//! Purpose: All text the settings window shows, in each supported language.
//! Created: 2026-11-08
//! Architecture: Each message must be translated to compile, so nothing can be left
//! half-finished. Layout names come from the system's own translations instead.

pub use keyway_core::language::Language;
use std::env;

const LANGUAGE_VARIABLES: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];
const LANGUAGE_PRIORITY_VARIABLE: &str = "LANGUAGE";
const LANGUAGE_LIST_SEPARATOR: char = ':';

/// The desktop's language, which Keyway's texts follow until the user chooses one. Follows the
/// same variables gettext does; anything unsupported gets English.
pub fn desktop_language() -> Language {
    let priority = env::var(LANGUAGE_PRIORITY_VARIABLE).unwrap_or_default();
    let from_priority = priority
        .split(LANGUAGE_LIST_SEPARATOR)
        .find_map(Language::from_locale);

    from_priority
        .or_else(|| {
            let first = LANGUAGE_VARIABLES
                .iter()
                .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()));

            first.and_then(|locale| Language::from_locale(&locale))
        })
        .unwrap_or(Language::English)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Message {
    WindowTitle,
    PageLayouts,
    PageShortcuts,
    PageAltCodes,
    YourLayouts,
    YourLayoutsNote,
    InUse,
    MoveUp,
    MoveDown,
    RemoveLayout,
    AddLayoutButton,
    Options,
    PerWindow,
    PerWindowNote,
    ShowFlags,
    ShowFlagsNote,
    KeyboardColors,
    KeyboardColorsNote,
    ThemeLight,
    ThemeDark,
    SystemKeyboard,
    SystemKeyboardNote,
    Open,
    OpenFailed,
    RestartKeyway,
    RestartKeywayNote,
    Restart,
    Restarted,
    RestartIncomplete,
    PartExtension,
    PartAgent,
    PartRouter,
    LoadExtensionTitle,
    LoadExtensionIntro,
    LoadExtensionSteps,
    CopyLine,
    LineCopied,
    LayoutLimitReached,
    SystemShortcutConflict,
    UseOnlyKeyway,
    Switching,
    SwitchingNote,
    NextLayout,
    PreviousLayout,
    ReleaseNote,
    SwitchingOrder,
    OrderList,
    OrderRecent,
    JumpToLayout,
    JumpToLayoutNote,
    NotSet,
    AltCodes,
    AltCodesNote,
    TypeByCode,
    NumLock,
    NumLockNote,
    NumLockIgnored,
    NumLockRequired,
    HexCodes,
    HexCodesNote,
    UnicodeCodes,
    UnicodeCodesNote,
    CodePages,
    CodePagesNote,
    AddLayoutTitle,
    Cancel,
    Add,
    Added,
    SearchPlaceholder,
    CaptureTitle,
    CapturePrompt,
    CaptureHint,
    ClearShortcut,
    UnknownKey,
    NeedsMoreKeys,
    ShortcutInUse,
    SaveFailed,
    ConfigInvalid,
    ConfigBackedUp,
}

/// Returns the message in a language. Translations by the author; the Greek and the Spanish
/// were proofread, but not yet by a native speaker.
pub const fn text(language: Language, message: Message) -> &'static str {
    use Language::{
        BrazilianPortuguese as Br, English as En, EuropeanPortuguese as Pt, Greek as El,
        Spanish as Es,
    };
    use Message as M;

    match message {
        M::WindowTitle => "Keyway",
        M::PageLayouts => match language {
            En => "Layouts",
            Es => "Distribuciones",
            Br => "Layouts",
            Pt => "Esquemas",
            El => "Διατάξεις",
        },
        M::PageShortcuts => match language {
            En => "Shortcuts",
            Es => "Atajos",
            Br | Pt => "Atalhos",
            El => "Συντομεύσεις",
        },
        M::PageAltCodes => match language {
            En => "Alt codes",
            Es => "Códigos Alt",
            Br | Pt => "Códigos Alt",
            El => "Κωδικοί Alt",
        },
        M::YourLayouts => match language {
            En => "Your layouts",
            Es => "Sus distribuciones",
            Br => "Seus layouts",
            Pt => "Os seus esquemas",
            El => "Οι διατάξεις σας",
        },
        M::YourLayoutsNote => match language {
            En => "Switching follows this order. The same list appears in the taskbar.",
            Es => "El cambio sigue este orden. La misma lista aparece en la barra de tareas.",
            Br => "A troca segue esta ordem. A mesma lista aparece na barra de tarefas.",
            Pt => "A troca segue esta ordem. A mesma lista aparece na barra de tarefas.",
            El => {
                "Η εναλλαγή ακολουθεί αυτή τη σειρά. Η ίδια λίστα εμφανίζεται στη γραμμή εργασιών."
            }
        },
        M::InUse => match language {
            En => "In use",
            Es => "En uso",
            Br | Pt => "Em uso",
            El => "Σε χρήση",
        },
        M::MoveUp => match language {
            En => "Move up",
            Es => "Subir",
            Br | Pt => "Mover para cima",
            El => "Μετακίνηση πάνω",
        },
        M::MoveDown => match language {
            En => "Move down",
            Es => "Bajar",
            Br | Pt => "Mover para baixo",
            El => "Μετακίνηση κάτω",
        },
        M::RemoveLayout => match language {
            En => "Remove layout",
            Es => "Quitar distribución",
            Br => "Remover layout",
            Pt => "Remover esquema",
            El => "Αφαίρεση διάταξης",
        },
        M::AddLayoutButton => match language {
            En => "Add layout…",
            Es => "Añadir distribución…",
            Br => "Adicionar layout…",
            Pt => "Adicionar esquema…",
            El => "Προσθήκη διάταξης…",
        },
        M::Options => match language {
            En => "Options",
            Es => "Opciones",
            Br | Pt => "Opções",
            El => "Επιλογές",
        },
        M::PerWindow => match language {
            En => "Remember the layout for each window",
            Es => "Recordar la distribución de cada ventana",
            Br => "Lembrar o layout de cada janela",
            Pt => "Lembrar o esquema de cada janela",
            El => "Απομνημόνευση της διάταξης για κάθε παράθυρο",
        },
        M::PerWindowNote => match language {
            En => "Each window keeps the layout you last used in it.",
            Es => "Cada ventana conserva la última distribución usada en ella.",
            Br => "Cada janela mantém o último layout usado nela.",
            Pt => "Cada janela mantém o último esquema usado nela.",
            El => "Κάθε παράθυρο κρατά τη διάταξη που χρησιμοποιήσατε τελευταία σε αυτό.",
        },
        M::ShowFlags => match language {
            En => "Show country flags",
            Es => "Mostrar banderas de países",
            Br | Pt => "Mostrar bandeiras dos países",
            El => "Εμφάνιση σημαιών χωρών",
        },
        M::ShowFlagsNote => match language {
            En => "Beside each layout's code, such as PT-BR, in the taskbar and here.",
            Es => "Junto al código de cada distribución, como ES-ES, en la barra de tareas y aquí.",
            Br => "Ao lado do código de cada layout, como PT-BR, na barra de tarefas e aqui.",
            Pt => "Ao lado do código de cada esquema, como PT-PT, na barra de tarefas e aqui.",
            El => "Δίπλα στον κωδικό κάθε διάταξης, όπως EL-GR, στη γραμμή εργασιών και εδώ.",
        },
        M::KeyboardColors => match language {
            En => "On-screen keyboard colors",
            Es => "Colores del teclado en pantalla",
            Br | Pt => "Cores do teclado virtual",
            El => "Χρώματα πληκτρολογίου οθόνης",
        },
        M::KeyboardColorsNote => match language {
            En => "The keyboard opens from Keyway's menu in the taskbar.",
            Es => "El teclado se abre desde el menú de Keyway en la barra de tareas.",
            Br | Pt => "O teclado abre pelo menu do Keyway na barra de tarefas.",
            El => "Το πληκτρολόγιο ανοίγει από το μενού του Keyway στη γραμμή εργασιών.",
        },
        M::ThemeLight => match language {
            En => "Light",
            Es => "Claro",
            Br | Pt => "Claro",
            El => "Φωτεινό",
        },
        M::ThemeDark => match language {
            En => "Dark",
            Es => "Oscuro",
            Br | Pt => "Escuro",
            El => "Σκούρο",
        },
        M::SystemKeyboard => match language {
            En => "System keyboard settings",
            Es => "Configuración del teclado del sistema",
            Br | Pt => "Configurações de teclado do sistema",
            El => "Ρυθμίσεις πληκτρολογίου του συστήματος",
        },
        M::SystemKeyboardNote => match language {
            En => "Shows this same list, plus typing options and the system's own shortcuts.",
            Es => {
                "Muestra esta misma lista, además de opciones de escritura y los atajos propios del sistema."
            }
            Br => "Mostra esta mesma lista, opções de digitação e os atalhos do próprio sistema.",
            Pt => "Mostra esta mesma lista, opções de escrita e os atalhos do próprio sistema.",
            El => {
                "Εμφανίζει την ίδια λίστα, καθώς και επιλογές πληκτρολόγησης και τις συντομεύσεις του ίδιου του συστήματος."
            }
        },
        M::Open => match language {
            En => "Open",
            Es => "Abrir",
            Br | Pt => "Abrir",
            El => "Άνοιγμα",
        },
        M::OpenFailed => match language {
            En => "Couldn't open the system's keyboard settings.",
            Es => "No se pudo abrir la configuración del teclado del sistema.",
            Br | Pt => "Não foi possível abrir as configurações de teclado do sistema.",
            El => "Δεν ήταν δυνατό το άνοιγμα των ρυθμίσεων πληκτρολογίου του συστήματος.",
        },
        M::RestartKeyway => match language {
            En => "Restart Keyway",
            Es => "Reiniciar Keyway",
            Br | Pt => "Reiniciar o Keyway",
            El => "Επανεκκίνηση του Keyway",
        },
        M::RestartKeywayNote => match language {
            En => {
                "Loads the installed version again, as after an update. It may ask for an administrator's password."
            }
            Es => {
                "Vuelve a cargar la versión instalada, como tras una actualización. Puede pedir la contraseña de administrador."
            }
            Br => {
                "Carrega de novo a versão instalada, como após uma atualização. Pode pedir a senha de administrador."
            }
            Pt => {
                "Carrega de novo a versão instalada, como após uma atualização. Pode pedir a palavra-passe de administrador."
            }
            El => {
                "Φορτώνει ξανά την εγκατεστημένη έκδοση, όπως μετά από μια ενημέρωση. Μπορεί να ζητήσει τον κωδικό πρόσβασης ενός διαχειριστή."
            }
        },
        M::Restart => match language {
            En => "Restart",
            Es => "Reiniciar",
            Br | Pt => "Reiniciar",
            El => "Επανεκκίνηση",
        },
        M::Restarted => match language {
            En => "Restarted",
            Es => "Reiniciado",
            Br | Pt => "Reiniciado",
            El => "Έγινε επανεκκίνηση",
        },
        M::RestartIncomplete => match language {
            En => "Not everything was restarted.",
            Es => "No se reinició todo.",
            Br | Pt => "Nem tudo foi reiniciado.",
            El => "Δεν έγινε επανεκκίνηση όλων.",
        },
        M::PartExtension => match language {
            En => "Desktop extension",
            Es => "Extensión del escritorio",
            Br => "Extensão da área de trabalho",
            Pt => "Extensão do ambiente de trabalho",
            El => "Επέκταση επιφάνειας εργασίας",
        },
        M::PartAgent => match language {
            En => "User service",
            Es => "Servicio de usuario",
            Br => "Serviço do usuário",
            Pt => "Serviço do utilizador",
            El => "Υπηρεσία χρήστη",
        },
        M::PartRouter => match language {
            En => "System service",
            Es => "Servicio del sistema",
            Br | Pt => "Serviço do sistema",
            El => "Υπηρεσία συστήματος",
        },
        M::LoadExtensionTitle => match language {
            En => "Load the extension",
            Es => "Cargar la extensión",
            Br | Pt => "Carregar a extensão",
            El => "Φόρτωση της επέκτασης",
        },
        M::LoadExtensionIntro => match language {
            En => {
                "GNOME Shell has not loaded this version of Keyway's extension yet. You can load it without ending your session:"
            }
            Es => {
                "GNOME Shell aún no ha cargado esta versión de la extensión de Keyway. Puede cargarla sin cerrar la sesión:"
            }
            Br => {
                "O GNOME Shell ainda não carregou esta versão da extensão do Keyway. Você pode carregá-la sem encerrar a sessão:"
            }
            Pt => {
                "O GNOME Shell ainda não carregou esta versão da extensão do Keyway. Pode carregá-la sem terminar a sessão:"
            }
            El => {
                "Το GNOME Shell δεν έχει φορτώσει ακόμη αυτή την έκδοση της επέκτασης του Keyway. Μπορείτε να τη φορτώσετε χωρίς να τερματίσετε τη συνεδρία:"
            }
        },
        M::LoadExtensionSteps => match language {
            En => {
                "1.  Copy the line below.\n2.  Press Alt+F2, type lg and press Enter.\n3.  Paste the line at the prompt, press Enter, then Esc."
            }
            Es => {
                "1.  Copie la línea de abajo.\n2.  Pulse Alt+F2, escriba lg y pulse Intro.\n3.  Pegue la línea en el cuadro de entrada, pulse Intro y después Esc."
            }
            Br => {
                "1.  Copie a linha abaixo.\n2.  Pressione Alt+F2, digite lg e pressione Enter.\n3.  Cole a linha no prompt, pressione Enter e depois Esc."
            }
            Pt => {
                "1.  Copie a linha abaixo.\n2.  Prima Alt+F2, escreva lg e prima Enter.\n3.  Cole a linha na linha de comandos, prima Enter e depois Esc."
            }
            El => {
                "1.  Αντιγράψτε την παρακάτω γραμμή.\n2.  Πατήστε Alt+F2, πληκτρολογήστε lg και πατήστε Enter.\n3.  Επικολλήστε τη γραμμή στην προτροπή, πατήστε Enter και μετά Esc."
            }
        },
        M::CopyLine => match language {
            En => "Copy line",
            Es => "Copiar línea",
            Br | Pt => "Copiar linha",
            El => "Αντιγραφή γραμμής",
        },
        M::LineCopied => match language {
            En => "Copied",
            Es => "Copiada",
            Br | Pt => "Copiada",
            El => "Αντιγράφηκε",
        },
        M::LayoutLimitReached => match language {
            En => "This desktop can't use more than four layouts.",
            Es => "Este escritorio no puede usar más de cuatro distribuciones.",
            Br => "Este ambiente não pode usar mais de quatro layouts.",
            Pt => "Este ambiente não pode usar mais de quatro esquemas.",
            El => {
                "Αυτή η επιφάνεια εργασίας δεν μπορεί να χρησιμοποιήσει πάνω από τέσσερις διατάξεις."
            }
        },
        M::SystemShortcutConflict => match language {
            En => {
                "The system has its own layout shortcut on modifier keys, so one press may switch twice."
            }
            Es => {
                "El sistema tiene su propio atajo para cambiar de distribución con teclas modificadoras, así que una pulsación puede cambiarla dos veces."
            }
            Br => {
                "O sistema tem um atalho próprio de layout em teclas modificadoras, então um toque pode trocar duas vezes."
            }
            Pt => {
                "O sistema tem um atalho próprio de esquema em teclas modificadoras, por isso um toque pode trocar duas vezes."
            }
            El => {
                "Το σύστημα έχει δική του συντόμευση αλλαγής διάταξης με πλήκτρα τροποποίησης, οπότε ένα πάτημα μπορεί να αλλάξει τη διάταξη δύο φορές."
            }
        },
        M::UseOnlyKeyway => match language {
            En => "Use only Keyway",
            Es => "Usar solo Keyway",
            Br | Pt => "Usar só o Keyway",
            El => "Χρήση μόνο του Keyway",
        },
        M::Switching => match language {
            En => "Switching",
            Es => "Cambio de distribución",
            Br | Pt => "Troca",
            El => "Εναλλαγή",
        },
        M::SwitchingNote => match language {
            En => "Select a shortcut to press a new one.",
            Es => "Seleccione un atajo para definir otro.",
            Br | Pt => "Selecione um atalho para definir outro.",
            El => "Επιλέξτε μια συντόμευση για να ορίσετε νέα.",
        },
        M::NextLayout => match language {
            En => "Next layout",
            Es => "Distribución siguiente",
            Br => "Próximo layout",
            Pt => "Esquema seguinte",
            El => "Επόμενη διάταξη",
        },
        M::PreviousLayout => match language {
            En => "Previous layout",
            Es => "Distribución anterior",
            Br => "Layout anterior",
            Pt => "Esquema anterior",
            El => "Προηγούμενη διάταξη",
        },
        M::ReleaseNote => match language {
            En => "Shortcuts without a final key fire when you release them.",
            Es => "Los atajos sin tecla final se activan al soltar las teclas.",
            Br | Pt => "Atalhos sem tecla final são acionados ao soltar as teclas.",
            El => "Οι συντομεύσεις χωρίς τελικό πλήκτρο ενεργοποιούνται όταν αφήσετε τα πλήκτρα.",
        },
        M::SwitchingOrder => match language {
            En => "Switching order",
            Es => "Orden de cambio",
            Br | Pt => "Ordem de troca",
            El => "Σειρά εναλλαγής",
        },
        M::OrderList => match language {
            En => "List order",
            Es => "Orden de la lista",
            Br | Pt => "Ordem da lista",
            El => "Σειρά της λίστας",
        },
        M::OrderRecent => match language {
            En => "Most recent first",
            Es => "La más reciente primero",
            Br | Pt => "Mais recente primeiro",
            El => "Πρώτα η πιο πρόσφατη",
        },
        M::JumpToLayout => match language {
            En => "Jump to a layout",
            Es => "Ir a una distribución",
            Br => "Ir para um layout",
            Pt => "Ir para um esquema",
            El => "Μετάβαση σε διάταξη",
        },
        M::JumpToLayoutNote => match language {
            En => {
                "Optional shortcuts that select one layout directly. A layout that has one is left out of the switching above."
            }
            Es => {
                "Atajos opcionales que seleccionan una distribución directamente. Una distribución con atajo propio queda fuera del cambio de arriba."
            }
            Br => {
                "Atalhos opcionais que selecionam um layout diretamente. Um layout com atalho próprio fica de fora da troca acima."
            }
            Pt => {
                "Atalhos opcionais que selecionam um esquema diretamente. Um esquema com atalho próprio fica de fora da troca acima."
            }
            El => {
                "Προαιρετικές συντομεύσεις που επιλέγουν απευθείας μια διάταξη. Μια διάταξη με δική της συντόμευση μένει εκτός της παραπάνω εναλλαγής."
            }
        },
        M::NotSet => match language {
            En => "Not set",
            Es => "Sin definir",
            Br | Pt => "Não definido",
            El => "Δεν έχει οριστεί",
        },
        M::AltCodes => match language {
            En => "Alt codes",
            Es => "Códigos Alt",
            Br | Pt => "Códigos Alt",
            El => "Κωδικοί Alt",
        },
        M::AltCodesNote => match language {
            En => "Hold Left Alt, type digits on the keypad and release Alt.",
            Es => {
                "Mantenga pulsada la tecla Alt izquierda, escriba los dígitos en el teclado numérico y suelte Alt."
            }
            Br | Pt => "Segure o Alt esquerdo, digite no teclado numérico e solte o Alt.",
            El => {
                "Κρατήστε πατημένο το αριστερό Alt, πληκτρολογήστε τα ψηφία στο αριθμητικό πληκτρολόγιο και αφήστε το Alt."
            }
        },
        M::TypeByCode => match language {
            En => "Type characters by code",
            Es => "Escribir caracteres por código",
            Br | Pt => "Digitar caracteres por código",
            El => "Πληκτρολόγηση χαρακτήρων με κωδικό",
        },
        M::NumLock => "NumLock",
        M::NumLockNote => match language {
            En => "When the keypad accepts codes.",
            Es => "Cuándo acepta códigos el teclado numérico.",
            Br | Pt => "Quando o teclado numérico aceita códigos.",
            El => "Πότε το αριθμητικό πληκτρολόγιο δέχεται κωδικούς.",
        },
        M::NumLockIgnored => match language {
            En => "On or off",
            Es => "Activado o desactivado",
            Br | Pt => "Ligado ou desligado",
            El => "Ενεργό ή ανενεργό",
        },
        M::NumLockRequired => match language {
            En => "Only when on",
            Es => "Solo cuando está activado",
            Br | Pt => "Só quando ligado",
            El => "Μόνο όταν είναι ενεργό",
        },
        M::HexCodes => match language {
            En => "Hexadecimal codes",
            Es => "Códigos hexadecimales",
            Br | Pt => "Códigos hexadecimais",
            El => "Δεκαεξαδικοί κωδικοί",
        },
        M::HexCodesNote => match language {
            En => "Alt, keypad plus, then a hex Unicode value.",
            Es => {
                "Alt, el signo más del teclado numérico y después un valor Unicode en hexadecimal."
            }
            Br | Pt => "Alt, mais do teclado numérico e um valor Unicode em hexadecimal.",
            El => {
                "Alt, το συν του αριθμητικού πληκτρολογίου και μετά μια δεκαεξαδική τιμή Unicode."
            }
        },
        M::UnicodeCodes => match language {
            En => "Unicode above 255",
            Es => "Unicode por encima de 255",
            Br | Pt => "Unicode acima de 255",
            El => "Unicode πάνω από 255",
        },
        M::UnicodeCodesNote => match language {
            En => "Windows wraps larger codes around 256; this types them as Unicode instead.",
            Es => {
                "Windows reduce los códigos mayores a su resto entre 256; esta opción los escribe como Unicode."
            }
            Br | Pt => {
                "O Windows reinicia códigos maiores a cada 256; isto os digita como Unicode."
            }
            El => {
                "Τα Windows κρατούν από τους μεγαλύτερους κωδικούς το υπόλοιπο της διαίρεσης με το 256· αυτή η επιλογή τούς γράφει ως Unicode."
            }
        },
        M::CodePages => match language {
            En => "Code pages",
            Es => "Páginas de códigos",
            Br | Pt => "Páginas de código",
            El => "Κωδικοσελίδες",
        },
        M::CodePagesNote => match language {
            En => "Each layout decides which characters its codes produce.",
            Es => "Cada distribución decide qué caracteres producen sus códigos.",
            Br => "Cada layout decide quais caracteres seus códigos produzem.",
            Pt => "Cada esquema decide que caracteres os seus códigos produzem.",
            El => "Κάθε διάταξη ορίζει ποιους χαρακτήρες δίνουν οι κωδικοί της.",
        },
        M::AddLayoutTitle => match language {
            En => "Add layout",
            Es => "Añadir distribución",
            Br => "Adicionar layout",
            Pt => "Adicionar esquema",
            El => "Προσθήκη διάταξης",
        },
        M::Cancel => match language {
            En => "Cancel",
            Es => "Cancelar",
            Br | Pt => "Cancelar",
            El => "Ακύρωση",
        },
        M::Add => match language {
            En => "Add",
            Es => "Añadir",
            Br | Pt => "Adicionar",
            El => "Προσθήκη",
        },
        M::Added => match language {
            En => "Added",
            Es => "Añadida",
            Br | Pt => "Adicionado",
            El => "Προστέθηκε",
        },
        M::SearchPlaceholder => match language {
            En => "Search layouts",
            Es => "Buscar distribuciones",
            Br => "Pesquisar layouts",
            Pt => "Pesquisar esquemas",
            El => "Αναζήτηση διατάξεων",
        },
        M::CaptureTitle => match language {
            En => "Set shortcut",
            Es => "Definir atajo",
            Br | Pt => "Definir atalho",
            El => "Ορισμός συντόμευσης",
        },
        M::CapturePrompt => match language {
            En => "Press the new shortcut",
            Es => "Pulse el nuevo atajo",
            Br | Pt => "Pressione o novo atalho",
            El => "Πατήστε τη νέα συντόμευση",
        },
        M::CaptureHint => match language {
            En => "Esc cancels.",
            Es => "Esc cancela.",
            Br | Pt => "Esc cancela.",
            El => "Το Esc ακυρώνει.",
        },
        M::ClearShortcut => match language {
            En => "Clear",
            Es => "Borrar",
            Br | Pt => "Limpar",
            El => "Εκκαθάριση",
        },
        M::UnknownKey => match language {
            En => "That key can't be used in a shortcut. Try another.",
            Es => "Esa tecla no se puede usar en un atajo. Pruebe otra.",
            Br | Pt => "Essa tecla não pode ser usada em um atalho. Tente outra.",
            El => "Αυτό το πλήκτρο δεν μπορεί να χρησιμοποιηθεί σε συντόμευση. Δοκιμάστε άλλο.",
        },
        M::NeedsMoreKeys => match language {
            En => "That would stop the key from typing. Add Ctrl, Alt or Super.",
            Es => "Eso impediría que la tecla escribiera. Añada Ctrl, Alt o Super.",
            Br | Pt => "Isso impediria a tecla de digitar. Adicione Ctrl, Alt ou Super.",
            El => "Έτσι το πλήκτρο θα έπαυε να γράφει. Προσθέστε Ctrl, Alt ή Super.",
        },
        M::ShortcutInUse => match language {
            En => "That shortcut is already in use. Try another.",
            Es => "Ese atajo ya está en uso. Pruebe otro.",
            Br | Pt => "Esse atalho já está em uso. Tente outro.",
            El => "Αυτή η συντόμευση χρησιμοποιείται ήδη. Δοκιμάστε άλλη.",
        },
        M::SaveFailed => match language {
            En => "Couldn't save your settings.",
            Es => "No se pudo guardar la configuración.",
            Br | Pt => "Não foi possível salvar as configurações.",
            El => "Δεν ήταν δυνατή η αποθήκευση των ρυθμίσεων.",
        },
        M::ConfigInvalid => match language {
            En => {
                "Your settings file has an error, so the defaults are shown. It's kept as a copy if you change anything here."
            }
            Es => {
                "El archivo de configuración tiene un error, así que se muestran los valores predeterminados. Se conservará una copia si cambia algo aquí."
            }
            Br | Pt => {
                "O arquivo de configurações tem um erro, então os padrões são mostrados. Uma cópia será mantida se você alterar algo aqui."
            }
            El => {
                "Το αρχείο ρυθμίσεων έχει σφάλμα, οπότε εμφανίζονται οι προεπιλογές. Θα κρατηθεί αντίγραφό του αν αλλάξετε κάτι εδώ."
            }
        },
        M::ConfigBackedUp => match language {
            En => "Your old settings file was kept as a copy.",
            Es => "El archivo de configuración anterior se conservó como copia.",
            Br | Pt => "O arquivo de configurações antigo foi mantido como cópia.",
            El => "Το παλιό αρχείο ρυθμίσεων κρατήθηκε ως αντίγραφο.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portuguese_variants_differ_where_they_should() {
        let brazilian = text(Language::BrazilianPortuguese, Message::PageLayouts);
        let european = text(Language::EuropeanPortuguese, Message::PageLayouts);

        assert_ne!(brazilian, european);
    }
}
