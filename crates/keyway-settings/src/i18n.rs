//! Module: keyway_settings::i18n
//! Purpose: All text the settings window shows, in each supported language.
//! Created: 2026-11-08
//! Architecture: Each message must be translated to compile, so nothing can be left
//! half-finished. Layout names come from the system's own translations instead.

use std::env;

const LANGUAGE_VARIABLES: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];
const LANGUAGE_PRIORITY_VARIABLE: &str = "LANGUAGE";
const LANGUAGE_LIST_SEPARATOR: char = ':';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    BrazilianPortuguese,
    EuropeanPortuguese,
    Greek,
}

impl Language {
    /// Follows the same variables gettext does; anything unsupported gets English.
    pub fn from_environment() -> Self {
        let priority = env::var(LANGUAGE_PRIORITY_VARIABLE).unwrap_or_default();
        let from_priority = priority
            .split(LANGUAGE_LIST_SEPARATOR)
            .find_map(Self::from_tag);

        from_priority
            .or_else(|| {
                let first = LANGUAGE_VARIABLES
                    .iter()
                    .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()));

                first.and_then(|tag| Self::from_tag(&tag))
            })
            .unwrap_or(Self::English)
    }

    /// Reads a locale tag such as pt_BR.UTF-8. Plain pt means Brazilian Portuguese, as
    /// that is how the other Keyway-related projects treat it.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let name = tag.split(['.', '@']).next().unwrap_or_default();
        let (language, region) = name.split_once(['_', '-']).unwrap_or((name, ""));

        match (language, region) {
            ("en", _) => Some(Self::English),
            ("pt", "PT") => Some(Self::EuropeanPortuguese),
            ("pt", _) => Some(Self::BrazilianPortuguese),
            ("el", _) => Some(Self::Greek),
            _ => None,
        }
    }
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

/// Returns the message in a language. Translations by the author; the Greek needs review
/// by a native speaker.
pub const fn text(language: Language, message: Message) -> &'static str {
    use Language::{
        BrazilianPortuguese as Br, English as En, EuropeanPortuguese as Pt, Greek as El,
    };
    use Message as M;

    match message {
        M::WindowTitle => "Keyway",
        M::PageLayouts => match language {
            En => "Layouts",
            Br => "Layouts",
            Pt => "Esquemas",
            El => "Διατάξεις",
        },
        M::PageShortcuts => match language {
            En => "Shortcuts",
            Br | Pt => "Atalhos",
            El => "Συντομεύσεις",
        },
        M::PageAltCodes => match language {
            En => "Alt codes",
            Br | Pt => "Códigos Alt",
            El => "Κωδικοί Alt",
        },
        M::YourLayouts => match language {
            En => "Your layouts",
            Br => "Seus layouts",
            Pt => "Os seus esquemas",
            El => "Οι διατάξεις σας",
        },
        M::YourLayoutsNote => match language {
            En => "Switching follows this order. The same list appears in the taskbar.",
            Br => "A troca segue esta ordem. A mesma lista aparece na barra de tarefas.",
            Pt => "A troca segue esta ordem. A mesma lista aparece na barra de tarefas.",
            El => {
                "Η εναλλαγή ακολουθεί αυτή τη σειρά. Η ίδια λίστα εμφανίζεται στη γραμμή εργασιών."
            }
        },
        M::InUse => match language {
            En => "In use",
            Br | Pt => "Em uso",
            El => "Σε χρήση",
        },
        M::MoveUp => match language {
            En => "Move up",
            Br | Pt => "Mover para cima",
            El => "Μετακίνηση πάνω",
        },
        M::MoveDown => match language {
            En => "Move down",
            Br | Pt => "Mover para baixo",
            El => "Μετακίνηση κάτω",
        },
        M::RemoveLayout => match language {
            En => "Remove layout",
            Br => "Remover layout",
            Pt => "Remover esquema",
            El => "Αφαίρεση διάταξης",
        },
        M::AddLayoutButton => match language {
            En => "Add layout…",
            Br => "Adicionar layout…",
            Pt => "Adicionar esquema…",
            El => "Προσθήκη διάταξης…",
        },
        M::Options => match language {
            En => "Options",
            Br | Pt => "Opções",
            El => "Επιλογές",
        },
        M::PerWindow => match language {
            En => "Remember the layout for each window",
            Br => "Lembrar o layout de cada janela",
            Pt => "Lembrar o esquema de cada janela",
            El => "Απομνημόνευση της διάταξης για κάθε παράθυρο",
        },
        M::PerWindowNote => match language {
            En => "Each window keeps the layout you last used in it.",
            Br => "Cada janela mantém o último layout usado nela.",
            Pt => "Cada janela mantém o último esquema usado nela.",
            El => "Κάθε παράθυρο κρατά τη διάταξη που χρησιμοποιήσατε τελευταία.",
        },
        M::ShowFlags => match language {
            En => "Show country flags",
            Br | Pt => "Mostrar bandeiras dos países",
            El => "Εμφάνιση σημαιών χωρών",
        },
        M::ShowFlagsNote => match language {
            En => "Beside each layout's code, such as PT-BR, in the taskbar and here.",
            Br => "Ao lado do código de cada layout, como PT-BR, na barra de tarefas e aqui.",
            Pt => "Ao lado do código de cada esquema, como PT-PT, na barra de tarefas e aqui.",
            El => "Δίπλα στον κωδικό κάθε διάταξης, όπως EL-GR, στη γραμμή εργασιών και εδώ.",
        },
        M::SystemKeyboard => match language {
            En => "System keyboard settings",
            Br | Pt => "Configurações de teclado do sistema",
            El => "Ρυθμίσεις πληκτρολογίου συστήματος",
        },
        M::SystemKeyboardNote => match language {
            En => "Shows this same list, plus typing options and the system's own shortcuts.",
            Br => "Mostra esta mesma lista, opções de digitação e os atalhos do próprio sistema.",
            Pt => "Mostra esta mesma lista, opções de escrita e os atalhos do próprio sistema.",
            El => {
                "Δείχνει την ίδια λίστα, επιλογές πληκτρολόγησης και τις συντομεύσεις του συστήματος."
            }
        },
        M::Open => match language {
            En => "Open",
            Br | Pt => "Abrir",
            El => "Άνοιγμα",
        },
        M::OpenFailed => match language {
            En => "Couldn't open the system's keyboard settings.",
            Br | Pt => "Não foi possível abrir as configurações de teclado do sistema.",
            El => "Δεν ήταν δυνατό το άνοιγμα των ρυθμίσεων πληκτρολογίου του συστήματος.",
        },
        M::RestartKeyway => match language {
            En => "Restart Keyway",
            Br | Pt => "Reiniciar o Keyway",
            El => "Επανεκκίνηση του Keyway",
        },
        M::RestartKeywayNote => match language {
            En => {
                "Loads the installed version again, as after an update. It may ask for an administrator's password."
            }
            Br => {
                "Carrega de novo a versão instalada, como após uma atualização. Pode pedir a senha de administrador."
            }
            Pt => {
                "Carrega de novo a versão instalada, como após uma atualização. Pode pedir a palavra-passe de administrador."
            }
            El => {
                "Φορτώνει ξανά την εγκατεστημένη έκδοση, όπως μετά από μια ενημέρωση. Μπορεί να ζητήσει τον κωδικό διαχειριστή."
            }
        },
        M::Restart => match language {
            En => "Restart",
            Br | Pt => "Reiniciar",
            El => "Επανεκκίνηση",
        },
        M::Restarted => match language {
            En => "Restarted",
            Br | Pt => "Reiniciado",
            El => "Έγινε επανεκκίνηση",
        },
        M::RestartIncomplete => match language {
            En => "Not everything was restarted.",
            Br | Pt => "Nem tudo foi reiniciado.",
            El => "Δεν έγινε επανεκκίνηση όλων.",
        },
        M::PartExtension => match language {
            En => "Desktop extension",
            Br => "Extensão da área de trabalho",
            Pt => "Extensão do ambiente de trabalho",
            El => "Επέκταση επιφάνειας εργασίας",
        },
        M::PartAgent => match language {
            En => "User service",
            Br => "Serviço do usuário",
            Pt => "Serviço do utilizador",
            El => "Υπηρεσία χρήστη",
        },
        M::PartRouter => match language {
            En => "System service",
            Br | Pt => "Serviço do sistema",
            El => "Υπηρεσία συστήματος",
        },
        M::LoadExtensionTitle => match language {
            En => "Load the extension",
            Br | Pt => "Carregar a extensão",
            El => "Φόρτωση της επέκτασης",
        },
        M::LoadExtensionIntro => match language {
            En => {
                "GNOME Shell has not loaded this version of Keyway's extension yet. You can load it without ending your session:"
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
            Br | Pt => "Copiar linha",
            El => "Αντιγραφή γραμμής",
        },
        M::LineCopied => match language {
            En => "Copied",
            Br | Pt => "Copiada",
            El => "Αντιγράφηκε",
        },
        M::LayoutLimitReached => match language {
            En => "This desktop can't use more than four layouts.",
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
            Br => {
                "O sistema tem um atalho próprio de layout em teclas modificadoras, então um toque pode trocar duas vezes."
            }
            Pt => {
                "O sistema tem um atalho próprio de esquema em teclas modificadoras, por isso um toque pode trocar duas vezes."
            }
            El => {
                "Το σύστημα έχει δική του συντόμευση διάταξης σε πλήκτρα τροποποίησης, οπότε ένα πάτημα μπορεί να αλλάξει δύο φορές."
            }
        },
        M::UseOnlyKeyway => match language {
            En => "Use only Keyway",
            Br | Pt => "Usar só o Keyway",
            El => "Μόνο το Keyway",
        },
        M::Switching => match language {
            En => "Switching",
            Br | Pt => "Troca",
            El => "Εναλλαγή",
        },
        M::SwitchingNote => match language {
            En => "Select a shortcut to press a new one.",
            Br | Pt => "Selecione um atalho para definir outro.",
            El => "Επιλέξτε μια συντόμευση για να ορίσετε νέα.",
        },
        M::NextLayout => match language {
            En => "Next layout",
            Br => "Próximo layout",
            Pt => "Esquema seguinte",
            El => "Επόμενη διάταξη",
        },
        M::PreviousLayout => match language {
            En => "Previous layout",
            Br => "Layout anterior",
            Pt => "Esquema anterior",
            El => "Προηγούμενη διάταξη",
        },
        M::ReleaseNote => match language {
            En => "Shortcuts without a final key fire when you release them.",
            Br | Pt => "Atalhos sem tecla final são acionados ao soltar as teclas.",
            El => "Οι συντομεύσεις χωρίς τελικό πλήκτρο ενεργοποιούνται όταν τα αφήσετε.",
        },
        M::SwitchingOrder => match language {
            En => "Switching order",
            Br | Pt => "Ordem de troca",
            El => "Σειρά εναλλαγής",
        },
        M::OrderList => match language {
            En => "List order",
            Br | Pt => "Ordem da lista",
            El => "Σειρά λίστας",
        },
        M::OrderRecent => match language {
            En => "Most recent first",
            Br | Pt => "Mais recente primeiro",
            El => "Πρώτα η πιο πρόσφατη",
        },
        M::JumpToLayout => match language {
            En => "Jump to a layout",
            Br => "Ir para um layout",
            Pt => "Ir para um esquema",
            El => "Μετάβαση σε διάταξη",
        },
        M::JumpToLayoutNote => match language {
            En => {
                "Optional shortcuts that select one layout directly. A layout that has one is left out of the switching above."
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
            Br | Pt => "Não definido",
            El => "Δεν έχει οριστεί",
        },
        M::AltCodes => match language {
            En => "Alt codes",
            Br | Pt => "Códigos Alt",
            El => "Κωδικοί Alt",
        },
        M::AltCodesNote => match language {
            En => "Hold Left Alt, type digits on the keypad and release Alt.",
            Br | Pt => "Segure o Alt esquerdo, digite no teclado numérico e solte o Alt.",
            El => {
                "Κρατήστε το αριστερό Alt, πληκτρολογήστε ψηφία στο αριθμητικό πληκτρολόγιο και αφήστε το."
            }
        },
        M::TypeByCode => match language {
            En => "Type characters by code",
            Br | Pt => "Digitar caracteres por código",
            El => "Πληκτρολόγηση χαρακτήρων με κωδικό",
        },
        M::NumLock => "NumLock",
        M::NumLockNote => match language {
            En => "When the keypad accepts codes.",
            Br | Pt => "Quando o teclado numérico aceita códigos.",
            El => "Πότε το αριθμητικό πληκτρολόγιο δέχεται κωδικούς.",
        },
        M::NumLockIgnored => match language {
            En => "On or off",
            Br | Pt => "Ligado ou desligado",
            El => "Ενεργό ή ανενεργό",
        },
        M::NumLockRequired => match language {
            En => "Only when on",
            Br | Pt => "Só quando ligado",
            El => "Μόνο όταν είναι ενεργό",
        },
        M::HexCodes => match language {
            En => "Hexadecimal codes",
            Br | Pt => "Códigos hexadecimais",
            El => "Δεκαεξαδικοί κωδικοί",
        },
        M::HexCodesNote => match language {
            En => "Alt, keypad plus, then a hex Unicode value.",
            Br | Pt => "Alt, mais do teclado numérico e um valor Unicode em hexadecimal.",
            El => "Alt, συν του αριθμητικού πληκτρολογίου και δεκαεξαδική τιμή Unicode.",
        },
        M::UnicodeCodes => match language {
            En => "Unicode above 255",
            Br | Pt => "Unicode acima de 255",
            El => "Unicode πάνω από 255",
        },
        M::UnicodeCodesNote => match language {
            En => "Windows wraps larger codes around 256; this types them as Unicode instead.",
            Br | Pt => {
                "O Windows reinicia códigos maiores a cada 256; isto os digita como Unicode."
            }
            El => {
                "Τα Windows αναδιπλώνουν τους μεγαλύτερους κωδικούς στο 256· αυτό τους γράφει ως Unicode."
            }
        },
        M::CodePages => match language {
            En => "Code pages",
            Br | Pt => "Páginas de código",
            El => "Κωδικοσελίδες",
        },
        M::CodePagesNote => match language {
            En => "Each layout decides which characters its codes produce.",
            Br => "Cada layout decide quais caracteres seus códigos produzem.",
            Pt => "Cada esquema decide que caracteres os seus códigos produzem.",
            El => "Κάθε διάταξη ορίζει ποιους χαρακτήρες δίνουν οι κωδικοί της.",
        },
        M::AddLayoutTitle => match language {
            En => "Add layout",
            Br => "Adicionar layout",
            Pt => "Adicionar esquema",
            El => "Προσθήκη διάταξης",
        },
        M::Cancel => match language {
            En => "Cancel",
            Br | Pt => "Cancelar",
            El => "Άκυρο",
        },
        M::Add => match language {
            En => "Add",
            Br | Pt => "Adicionar",
            El => "Προσθήκη",
        },
        M::Added => match language {
            En => "Added",
            Br | Pt => "Adicionado",
            El => "Προστέθηκε",
        },
        M::SearchPlaceholder => match language {
            En => "Search layouts",
            Br => "Pesquisar layouts",
            Pt => "Pesquisar esquemas",
            El => "Αναζήτηση διατάξεων",
        },
        M::CaptureTitle => match language {
            En => "Set shortcut",
            Br | Pt => "Definir atalho",
            El => "Ορισμός συντόμευσης",
        },
        M::CapturePrompt => match language {
            En => "Press the new shortcut",
            Br | Pt => "Pressione o novo atalho",
            El => "Πατήστε τη νέα συντόμευση",
        },
        M::CaptureHint => match language {
            En => "Esc cancels.",
            Br | Pt => "Esc cancela.",
            El => "Το Esc ακυρώνει.",
        },
        M::ClearShortcut => match language {
            En => "Clear",
            Br | Pt => "Limpar",
            El => "Εκκαθάριση",
        },
        M::UnknownKey => match language {
            En => "That key can't be used in a shortcut. Try another.",
            Br | Pt => "Essa tecla não pode ser usada em um atalho. Tente outra.",
            El => "Αυτό το πλήκτρο δεν μπορεί να χρησιμοποιηθεί. Δοκιμάστε άλλο.",
        },
        M::NeedsMoreKeys => match language {
            En => "That would stop the key from typing. Add Ctrl, Alt or Super.",
            Br | Pt => "Isso impediria a tecla de digitar. Adicione Ctrl, Alt ou Super.",
            El => "Έτσι το πλήκτρο δεν θα πληκτρολογεί. Προσθέστε Ctrl, Alt ή Super.",
        },
        M::ShortcutInUse => match language {
            En => "That shortcut is already in use. Try another.",
            Br | Pt => "Esse atalho já está em uso. Tente outro.",
            El => "Αυτή η συντόμευση χρησιμοποιείται ήδη. Δοκιμάστε άλλη.",
        },
        M::SaveFailed => match language {
            En => "Couldn't save your settings.",
            Br | Pt => "Não foi possível salvar as configurações.",
            El => "Δεν ήταν δυνατή η αποθήκευση των ρυθμίσεων.",
        },
        M::ConfigInvalid => match language {
            En => {
                "Your settings file has an error, so the defaults are shown. It's kept as a copy if you change anything here."
            }
            Br | Pt => {
                "O arquivo de configurações tem um erro, então os padrões são mostrados. Uma cópia será mantida se você alterar algo aqui."
            }
            El => {
                "Το αρχείο ρυθμίσεων έχει σφάλμα, οπότε εμφανίζονται οι προεπιλογές. Θα κρατηθεί αντίγραφο αν αλλάξετε κάτι εδώ."
            }
        },
        M::ConfigBackedUp => match language {
            En => "Your old settings file was kept as a copy.",
            Br | Pt => "O arquivo de configurações antigo foi mantido como cópia.",
            El => "Το παλιό αρχείο ρυθμίσεων κρατήθηκε ως αντίγραφο.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_tags_choose_the_right_language() {
        assert_eq!(
            Language::from_tag("pt_BR.UTF-8"),
            Some(Language::BrazilianPortuguese)
        );
        assert_eq!(
            Language::from_tag("pt"),
            Some(Language::BrazilianPortuguese)
        );
        assert_eq!(
            Language::from_tag("pt_PT"),
            Some(Language::EuropeanPortuguese)
        );
        assert_eq!(
            Language::from_tag("pt_PT@euro"),
            Some(Language::EuropeanPortuguese)
        );
        assert_eq!(Language::from_tag("el_GR.UTF-8"), Some(Language::Greek));
        assert_eq!(Language::from_tag("en_US"), Some(Language::English));
        assert_eq!(Language::from_tag("ja_JP"), None);
        assert_eq!(Language::from_tag(""), None);
    }

    #[test]
    fn the_portuguese_variants_differ_where_they_should() {
        let brazilian = text(Language::BrazilianPortuguese, Message::PageLayouts);
        let european = text(Language::EuropeanPortuguese, Message::PageLayouts);

        assert_ne!(brazilian, european);
    }
}
