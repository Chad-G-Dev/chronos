const bordered_el = document.querySelectorAll('.bordered');

// function to add a border to an element
let add_border = (el) => {
    el.classList.add('bordered');

    const tl = document.createElement('i');
    const tr = document.createElement('i');
    const bl = document.createElement('i');
    const br = document.createElement('i');

    tl.className = 'corner tl';
    tr.className = 'corner tr';
    bl.className = 'corner bl';
    br.className = 'corner br';

    el.prepend(tl, tr, bl, br);
}

for (let el of bordered_el) {
    add_border(el);
}